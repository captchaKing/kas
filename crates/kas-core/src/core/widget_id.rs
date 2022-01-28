// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Widget identifiers

// x << a + b is x << (a + b)
#![allow(clippy::precedence)]

use crate::cast::{Cast, Conv};
use std::cmp::{Eq, PartialEq};
use std::hash::{Hash, Hasher};
use std::iter::once;
use std::marker::PhantomData;
use std::mem::size_of;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::{fmt, slice};

/// Invalid (default) identifier
const INVALID: u64 = !0;

/// `x & USE_BITS != 0`: rest is a sequence of 4-bit blocks; len is number of blocks used
const USE_BITS: u64 = 0x01;
/// Set when using a pointer as a safety feature.
const USE_PTR: usize = 0x02;

const MASK_LEN: u64 = 0xF0;
const SHIFT_LEN: u8 = 4;
const BLOCKS: u8 = 14;
const MASK_BITS: u64 = 0xFFFF_FFFF_FFFF_FF00;

#[cfg(target_pointer_width = "32")]
const MASK_PTR: usize = 0xFFFF_FFFC;
#[cfg(target_pointer_width = "64")]
const MASK_PTR: usize = 0xFFFF_FFFF_FFFF_FFFC;

/// Integer or pointer to a reference-counted slice.
///
/// Use `Self::get_ptr` to determine the variant used. When reading a pointer,
/// mask with MASK_PTR.
///
/// `self.0 & USE_BITS` is the "flag bit" determining the variant used. This
/// overlaps with the pointer's lowest bit (which must be zero due to alignment).
///
/// `PhantomData<Rc<()>>` is used to impl !Send and !Sync. We need atomic
/// reference counting to support those.
struct IntOrPtr(NonZeroU64, PhantomData<Rc<()>>);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Variant<'a> {
    Invalid,
    Int(u64),
    Slice(&'a [usize]),
}

impl IntOrPtr {
    const ROOT: Self = IntOrPtr(unsafe { NonZeroU64::new_unchecked(USE_BITS) }, PhantomData);
    const INVALID: Self = IntOrPtr(unsafe { NonZeroU64::new_unchecked(INVALID) }, PhantomData);

    #[inline]
    fn get_ptr(&self) -> Option<*mut usize> {
        if self.0.get() & USE_BITS == 0 {
            let p = usize::conv(self.0.get()) & MASK_PTR;
            Some(p as *mut usize)
        } else {
            None
        }
    }

    /// Construct from an integer
    ///
    /// Note: requires `x & USE_BITS != 0`.
    fn new_int(x: u64) -> Self {
        assert!(x & USE_BITS != 0);
        let x = NonZeroU64::new(x).unwrap();
        let u = IntOrPtr(x, PhantomData);
        assert!(u.get_ptr().is_none());
        u
    }

    /// Construct as a slice from an iterator
    fn new_iter<I: Clone + Iterator<Item = usize>>(iter: I) -> Self {
        let ref_count = 1;
        let len = iter.clone().count();
        let v: Vec<usize> = once(ref_count).chain(once(len)).chain(iter).collect();
        let b = v.into_boxed_slice();
        let p = Box::leak(b) as *mut [usize] as *mut usize;
        let p = p as usize;
        debug_assert_eq!(p & 3, 0);
        let p = p | USE_PTR;
        let u = IntOrPtr(NonZeroU64::new(p.cast()).unwrap(), PhantomData);
        assert!(u.get_ptr().is_some());
        u
    }

    fn get(&self) -> Variant {
        unsafe {
            if let Some(p) = self.get_ptr() {
                let len = *p.offset(1);
                let p = p.offset(2);
                let slice = slice::from_raw_parts(p, len);
                Variant::Slice(slice)
            } else if self.0.get() == INVALID {
                Variant::Invalid
            } else {
                Variant::Int(self.0.get())
            }
        }
    }

    fn as_u64(&self) -> u64 {
        self.0.get()
    }

    // Compatible with values geneated by `Self::as_u64`
    unsafe fn opt_from_u64(n: u64) -> Option<IntOrPtr> {
        if n == 0 {
            None
        } else {
            // We expect either USE_BITS or USE_PTR here; anything else indicates an error
            let v = n & 3;
            assert!(v == 1 || v == 2, "WidgetId::opt_from_u64: invalid value");
            let x = NonZeroU64::new(n).unwrap();
            Some(IntOrPtr(x, PhantomData))
        }
    }
}

impl Clone for IntOrPtr {
    fn clone(&self) -> Self {
        if let Some(p) = self.get_ptr() {
            unsafe {
                let ref_count = *p;

                // Copy behaviour of Rc::clone:
                if ref_count == 0 || ref_count == usize::MAX {
                    std::process::abort();
                }

                *p = ref_count + 1;
            }
        }
        IntOrPtr(self.0, PhantomData)
    }
}

impl Drop for IntOrPtr {
    fn drop(&mut self) {
        if let Some(p) = self.get_ptr() {
            unsafe {
                let ref_count = *p;
                if ref_count > 1 {
                    *p = ref_count - 1;
                } else {
                    // len+2 because path len does not include ref_count or len "fields"
                    let len = *p.offset(1) + 2;
                    let slice = slice::from_raw_parts_mut(p, len);
                    let _ = Box::<[usize]>::from_raw(slice);
                }
            }
        }
    }
}

enum PathIter<'a> {
    Bits(BitsIter),
    Slice(std::iter::Cloned<std::slice::Iter<'a, usize>>),
}

impl<'a> Iterator for PathIter<'a> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        match self {
            PathIter::Bits(bits) => bits.next(),
            PathIter::Slice(slice) => slice.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            PathIter::Bits(bits) => bits.size_hint(),
            PathIter::Slice(slice) => slice.size_hint(),
        }
    }
}

/// Widget identifier
///
/// All widgets are assigned an identifier which is unique within the window.
/// This type may be tested for equality, ancestry, and to determine the "index"
/// of a child.
///
/// This type is small (64-bit) and non-zero: `Option<WidgetId>` has the same
/// size as `WidgetId`. It is also very cheap to `Clone`: usually only one `if`
/// check, and in the worst case a pointer dereference and ref-count increment.
///
/// `WidgetId` is neither `Send` nor `Sync` since it may use non-atomic
/// reference counting internally.
///
/// Identifiers are assigned when configured and when re-configured
/// (via [`crate::TkAction::RECONFIGURE`]). Since user-code is not notified of a
/// re-configure, user-code should not store a `WidgetId`.
#[derive(Clone)]
pub struct WidgetId(IntOrPtr);

// Encode lowest 48 bits of index into the low bits of a u64, returning also the encoded bit-length
fn encode(index: usize) -> (u64, u8) {
    debug_assert!(8 * size_of::<usize>() as u32 - index.leading_zeros() <= 64);
    let mut x = index as u64 & 0x0000_FFFF_FFFF_FFFF;
    let mut y = x & 7;
    x >>= 3;
    let mut shift = 4;
    while x != 0 {
        y |= (8 | (x & 7)) << shift;
        x >>= 3;
        shift += 4;
    }
    (y, shift)
}

#[inline]
fn block_len(x: u64) -> u8 {
    ((x & MASK_LEN) >> SHIFT_LEN) as u8
}

// Returns usize read from highest bits of x plus blocks used
fn next_from_bits(mut x: u64) -> (usize, u8) {
    const TAKE: u64 = 0x7000_0000_0000_0000;
    const HIGH: u64 = 0x8000_0000_0000_0000;
    let mut y = (x & TAKE) >> 60;
    let mut c = 1;
    while (x & HIGH) != 0 {
        x <<= 4;
        y = (y << 3) | ((x & TAKE) >> 60);
        c += 1;
    }
    (y.cast(), c)
}

#[derive(Clone, Debug)]
struct BitsIter(u8, u64);
impl BitsIter {
    fn new(bits: u64) -> Self {
        assert!((bits & USE_BITS) != 0);
        let len = block_len(bits);
        BitsIter(len as u8, bits & MASK_BITS)
    }
}
impl Iterator for BitsIter {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.0 == 0 {
            return None;
        }
        let (next, blocks) = next_from_bits(self.1);
        self.0 -= blocks;
        self.1 <<= 4 * blocks;
        Some(next)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        ((self.0 != 0) as usize, Some(self.0 as usize))
    }
}

impl WidgetId {
    /// Identifier of the window
    pub(crate) const ROOT: Self = WidgetId(IntOrPtr::ROOT);

    const INVALID: Self = WidgetId(IntOrPtr::INVALID);

    /// Is the identifier valid?
    ///
    /// Default-constructed identifiers are invalid. Comparing invalid ids is
    /// considered a logic error and thus will panic in debug builds.
    /// This method may be used to check an identifier's validity.
    pub fn is_valid(&self) -> bool {
        self.0.get() != Variant::Invalid
    }

    /// Returns true if `self` equals `id` or if `id` is a descendant of `self`
    pub fn is_ancestor_of(&self, id: &Self) -> bool {
        match (self.0.get(), id.0.get()) {
            (Variant::Invalid, _) | (_, Variant::Invalid) => false,
            (Variant::Slice(_), Variant::Int(_)) => {
                // This combo will never be created where id is a child.
                false
            }
            (Variant::Int(self_x), Variant::Int(child_x)) => {
                let self_blocks = block_len(self_x);
                let child_blocks = block_len(child_x);
                if self_blocks > child_blocks {
                    return false;
                }

                // self_blocks == 0 for ROOT, otherwise > 0
                let shift = 4 * (BLOCKS - self_blocks) + 8;
                shift == 64 || self_x >> shift == child_x >> shift
            }
            (Variant::Int(self_x), Variant::Slice(child)) => {
                let iter = BitsIter::new(self_x);
                iter.zip(child.iter()).all(|(a, b)| a == *b)
            }
            (Variant::Slice(self_path), Variant::Slice(child)) => child.starts_with(self_path),
        }
    }

    /// Get index of `child` relative to `self`
    ///
    /// Returns `None` if `child` is not a descendant of `self`.
    pub fn index_of_child(&self, child: &Self) -> Option<usize> {
        match (self.0.get(), child.0.get()) {
            (Variant::Invalid, _) | (_, Variant::Invalid) => None,
            (Variant::Slice(_), Variant::Int(_)) => None,
            (Variant::Int(self_x), Variant::Int(child_x)) => {
                let self_blocks = block_len(self_x);
                let child_blocks = block_len(child_x);
                if self_blocks >= child_blocks {
                    return None;
                }

                // self_blocks == 0 for ROOT, otherwise > 0
                let shift = 4 * (BLOCKS - self_blocks) + 8;
                if shift != 64 && self_x >> shift != child_x >> shift {
                    return None;
                }

                debug_assert!(child_blocks > 0);
                let next_bits = (child_x & MASK_BITS) << (4 * self_blocks);
                Some(next_from_bits(next_bits).0)
            }
            (Variant::Int(self_x), Variant::Slice(child_path)) => {
                let iter = BitsIter::new(self_x);
                let mut child_iter = child_path.iter();
                if iter.zip(&mut child_iter).all(|(a, b)| a == *b) {
                    child_iter.next().cloned()
                } else {
                    None
                }
            }
            (Variant::Slice(self_path), Variant::Slice(child_path)) => {
                if child_path.starts_with(self_path) {
                    child_path[self_path.len()..].iter().next().cloned()
                } else {
                    None
                }
            }
        }
    }

    /// Make an identifier for the child with the given `index`
    ///
    /// Note: this is not a getter method. Calling multiple times with the same
    /// `index` may or may not return the same value!
    #[must_use]
    pub fn make_child(&self, index: usize) -> Self {
        match self.0.get() {
            Variant::Invalid => panic!("WidgetId::make_child: invalid id"),
            Variant::Int(self_x) => {
                // TODO(opt): this bit-packing approach is designed for space-optimisation, but it may
                // be better to use a simpler, less-compressed approach, possibly with u128 type.
                let block_len = block_len(self_x);
                let avail_blocks = BLOCKS - block_len;
                // Note: zero is encoded with 1 block to force bump to len
                let req_bits = (8 * size_of::<usize>() as u8 - index.leading_zeros() as u8).max(1);
                if req_bits <= 3 * avail_blocks {
                    let (bits, bit_len) = encode(index);
                    let used_blocks = bit_len / 4;
                    debug_assert_eq!(used_blocks, (req_bits + 2) / 3);
                    let len = (block_len as u64 + used_blocks as u64) << SHIFT_LEN;
                    let rest = bits << 4 * avail_blocks - bit_len + 8;
                    let id = (self_x & MASK_BITS) | rest | len | USE_BITS;
                    WidgetId(IntOrPtr::new_int(id))
                } else {
                    WidgetId(IntOrPtr::new_iter(BitsIter::new(self_x).chain(once(index))))
                }
            }
            Variant::Slice(path) => {
                WidgetId(IntOrPtr::new_iter(path.iter().cloned().chain(once(index))))
            }
        }
    }

    /// Convert to a `u64`
    ///
    /// This value should not be interpreted, except as follows:
    ///
    /// -   it is guaranteed non-zero
    /// -   it may be passed to [`Self::opt_from_u64`]
    /// -   comparing two `u64` values generated this way will mostly work as
    ///     an equality check of the source [`WidgetId`], but can return false
    ///     negatives (only if each id was generated through separate calls to
    ///     [`Self::make_child`])
    pub fn as_u64(&self) -> u64 {
        self.0.as_u64()
    }

    /// Convert `Option<WidgetId>` to `u64`
    ///
    /// This value should not be interpreted, except as follows:
    ///
    /// -   it is zero if and only if `id == None`
    /// -   it may be passed to [`Self::opt_from_u64`]
    /// -   comparing two `u64` values generated this way will mostly work as
    ///     an equality check of the source [`WidgetId`], but can return false
    ///     negatives (only if each id was generated through separate calls to
    ///     [`Self::make_child`])
    pub fn opt_to_u64(id: Option<&WidgetId>) -> u64 {
        match id {
            None => 0,
            Some(id) => id.0.as_u64(),
        }
    }

    /// Convert `u64` to `Option<WidgetId>`
    ///
    /// # Safety
    ///
    /// This may only be called with the output of [`Self::as_u64`],
    /// [`Self::opt_from_u64`], or `0`.
    ///
    /// This always "succeeds", though the result may not identify any widget.
    pub unsafe fn opt_from_u64(n: u64) -> Option<WidgetId> {
        IntOrPtr::opt_from_u64(n).map(WidgetId)
    }

    /// Construct an iterator, returning indices
    ///
    /// This represents the widget's "path" from the root (window).
    pub fn iter_path(&self) -> impl Iterator<Item = usize> + '_ {
        match self.0.get() {
            Variant::Invalid => panic!("WidgetId::iter_path on invalid"),
            Variant::Int(x) => PathIter::Bits(BitsIter::new(x)),
            Variant::Slice(path) => PathIter::Slice(path.iter().cloned()),
        }
    }
}

impl PartialEq for WidgetId {
    fn eq(&self, rhs: &Self) -> bool {
        match (self.0.get(), rhs.0.get()) {
            (Variant::Invalid, _) | (_, Variant::Invalid) => panic!("WidgetId::eq: invalid id"),
            (Variant::Int(x), Variant::Int(y)) => x == y,
            _ => self.iter_path().eq(rhs.iter_path()),
        }
    }
}
impl Eq for WidgetId {}

impl Hash for WidgetId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.0.get() {
            Variant::Invalid => (),
            Variant::Int(x) => {
                x.hash(state);
            }
            Variant::Slice(path) => {
                path.hash(state);
            }
        }
    }
}

impl PartialEq<Option<WidgetId>> for WidgetId {
    #[inline]
    fn eq(&self, rhs: &Option<WidgetId>) -> bool {
        rhs.as_ref().map(|id| id == self).unwrap_or(false)
    }
}

impl<'a> PartialEq<Option<&'a WidgetId>> for WidgetId {
    #[inline]
    fn eq(&self, rhs: &Option<&'a WidgetId>) -> bool {
        rhs.map(|id| id == self).unwrap_or(false)
    }
}

impl<'a> PartialEq<&'a WidgetId> for WidgetId {
    #[inline]
    fn eq(&self, rhs: &&WidgetId) -> bool {
        self == *rhs
    }
}

impl<'a> PartialEq<&'a Option<WidgetId>> for WidgetId {
    #[inline]
    fn eq(&self, rhs: &&Option<WidgetId>) -> bool {
        self == *rhs
    }
}

impl Default for WidgetId {
    fn default() -> Self {
        WidgetId::INVALID
    }
}

impl fmt::Debug for WidgetId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "WidgetId({})", self)
    }
}

impl fmt::Display for WidgetId {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.0.get() {
            Variant::Invalid => write!(f, "#INVALID"),
            Variant::Int(x) => {
                let len = block_len(x);
                if len == 0 {
                    write!(f, "#")
                } else {
                    let bits = x >> (4 * (BLOCKS - len) + 8);
                    write!(f, "#{1:0>0$x}", len as usize, bits)
                }
            }
            Variant::Slice(path) => {
                write!(f, "#")?;
                for index in path {
                    let (bits, bit_len) = encode(*index);
                    write!(f, "{1:0>0$x}", bit_len as usize / 4, bits)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn size_of_option_widget_id() {
        use std::mem::size_of;
        assert_eq!(size_of::<WidgetId>(), 8);
        assert_eq!(size_of::<WidgetId>(), size_of::<Option<WidgetId>>());
    }

    #[test]
    fn test_partial_eq() {
        assert_eq!(WidgetId::ROOT, WidgetId::ROOT);
        let c1 = WidgetId::ROOT.make_child(0).make_child(15);
        let c2 = WidgetId::ROOT.make_child(1).make_child(15);
        let c3 = WidgetId::ROOT.make_child(0).make_child(14);
        let c4 = WidgetId::ROOT.make_child(0).make_child(15);
        println!("c1: {}", c1);
        assert!(c1 != c2);
        assert!(c1 != c3);
        assert!(c2 != c3);
        assert_eq!(c1, c4);
        assert!(c1 != WidgetId::ROOT);

        let d1 = WidgetId(IntOrPtr::new_iter([0, 15].iter().cloned()));
        let d2 = WidgetId(IntOrPtr::new_iter([1, 15].iter().cloned()));
        assert_eq!(c1, d1);
        assert_eq!(c2, d2);
        assert!(d1 != d2);
        assert!(d1 != WidgetId::ROOT);
    }

    #[test]
    #[should_panic]
    fn test_partial_eq_invalid_1() {
        assert_eq!(WidgetId::INVALID, WidgetId::INVALID);
    }

    #[test]
    #[should_panic]
    fn test_partial_eq_invalid_2() {
        assert_eq!(WidgetId::ROOT, WidgetId::INVALID);
    }

    #[test]
    #[should_panic]
    fn test_partial_eq_invalid_3() {
        assert_eq!(WidgetId::INVALID, WidgetId::ROOT);
    }

    #[test]
    fn test_next_from_bits() {
        const REST: u64 = 0x0FFF_FFFF_FFFF_FFFF;
        assert_eq!(next_from_bits(0), (0, 1));
        assert_eq!(next_from_bits(REST), (0, 1));
        assert_eq!(next_from_bits((7 << 60) | REST), (7, 1));
        assert_eq!(next_from_bits((0xB << 60) | (3 << 56)), (27, 2));
        assert_eq!(
            next_from_bits(0xC9A4_F300_0000_0000),
            ((4 << 9) + (1 << 6) + (2 << 3) + 4, 4)
        );
    }

    #[test]
    fn text_bits_iter() {
        fn as_vec(x: u64) -> Vec<usize> {
            BitsIter::new(x).collect()
        }
        assert_eq!(as_vec(USE_BITS), Vec::<usize>::new());
        assert_eq!(as_vec(0x3100_0000_0000_0011), vec![3]);
        assert_eq!(as_vec(0x1A93_007F_0000_0071), vec![1, 139, 0, 0, 7]);
    }

    #[test]
    fn test_make_child() {
        fn test(seq: &[usize], x: u64) {
            let mut id = WidgetId::ROOT;
            for index in seq {
                id = id.make_child(*index);
            }
            let v = id.as_u64();
            if v != x {
                panic!("test({:?}, {:x}): found {:x}", seq, x, v);
            }

            // Every id is its own ancestor:
            assert!(id.is_ancestor_of(&id));
        }

        test(&[], USE_BITS);
        test(&[0, 0, 0], (3 << 4) | USE_BITS);
        test(&[0, 1, 0], (3 << 4) | (1 << 56) | USE_BITS);
        test(&[9, 0, 1, 300], 0x9101cd4000000071);
        test(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0], 0x12345679091920e1);
    }

    #[test]
    fn test_display() {
        fn from_seq(seq: &[usize]) -> String {
            let mut id = WidgetId::ROOT;
            for x in seq {
                id = id.make_child(*x);
            }
            format!("{}", id)
        }

        assert_eq!(from_seq(&[]), "#");
        assert_eq!(from_seq(&[0]), "#0");
        assert_eq!(from_seq(&[1, 2, 3]), "#123");
        assert_eq!(from_seq(&[5, 9, 13]), "#59195");
        assert_eq!(from_seq(&[321]), "#d81");
        assert_eq!(
            from_seq(&[313553, 13513, 13511631]),
            "#99ccba1bab91ebcadf97"
        )
    }

    #[test]
    fn test_is_ancestor() {
        fn test(seq: &[usize], seq2: &[usize]) {
            let mut id = WidgetId::ROOT;
            for x in seq {
                id = id.make_child(*x);
            }
            println!("id={} val={:x} from {:?}", id, id.as_u64(), seq);
            let mut id2 = id.clone();
            for x in seq2 {
                id2 = id2.make_child(*x);
            }
            println!("id2={} val={:x} from {:?}", id2, id2.as_u64(), seq2);
            let next = seq2.iter().next().cloned();
            assert_eq!(id.is_ancestor_of(&id2), next.is_some() || id == id2);
            assert_eq!(id.index_of_child(&id2), next);
        }

        test(&[], &[]);
        test(&[], &[1]);
        test(&[], &[51930, 2, 18]);
        test(&[5, 6, 0, 1, 1], &[]);
        test(&[5, 6, 0, 1, 1], &[1, 1]);
        test(&[8, 26], &[0]);
        test(&[9, 9, 9, 9, 9, 9, 9], &[]);
        test(&[9, 9, 9, 9, 9, 9, 9], &[6]);
        test(&[9, 9, 9, 9, 9, 9, 9, 9], &[3]);
        test(&[0, 2, 2, 0, 17], &[0]);
    }

    #[test]
    fn test_not_ancestor() {
        fn test(seq: &[usize], seq2: &[usize]) {
            let mut id = WidgetId::ROOT;
            for x in seq {
                id = id.make_child(*x);
            }
            println!("id={} val={:x} from {:?}", id, id.as_u64(), seq);
            let mut id2 = WidgetId::ROOT;
            for x in seq2 {
                id2 = id2.make_child(*x);
            }
            println!("id2={} val={:x} from {:?}", id2, id2.as_u64(), seq2);
            assert_eq!(id.is_ancestor_of(&id2), false);
            assert_eq!(id.index_of_child(&id2), None);
        }

        test(&[0], &[]);
        test(&[2, 10, 1], &[2, 10]);
    }
}