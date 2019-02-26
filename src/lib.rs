// Portions Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.

//! Provides a wrapper for allocating, handling and interacting with the guest memory regions.

#![warn(missing_docs)]

extern crate libc;

use std::io::{Read, Write};
use std::mem::size_of;
use std::result::Result;
use std::slice::{from_raw_parts, from_raw_parts_mut};

#[macro_use]
mod address_space;
pub use address_space::*;

pub mod endian;
pub use endian::*;

pub mod guest_memory;
pub use guest_memory::*;

#[cfg(feature = "memory-backend-mmap")]
pub mod mmap;
#[cfg(feature = "memory-backend-mmap")]
pub use mmap::*;

pub mod volatile_memory;
pub use volatile_memory::*;

/// Types for which it is safe to initialize from raw data.
///
/// A type `T` is `DataInit` if and only if it can be initialized by reading its contents from a
/// byte array.  This is generally true for all plain-old-data structs.  It is notably not true for
/// any type that includes a reference.
///
/// Implementing this trait guarantees that it is safe to instantiate the struct with random data.
pub unsafe trait DataInit: Copy + Send + Sync {
    /// Converts a slice of raw data into a reference of `Self`.
    ///
    /// The value of `data` is not copied. Instead a reference is made from the given slice. The
    /// value of `Self` will depend on the representation of the type in memory, and may change in
    /// an unstable fashion.
    ///
    /// This will return `None` if the length of data does not match the size of `Self`, or if the
    /// data is not aligned for the type of `Self`.
    fn from_slice(data: &[u8]) -> Option<&Self> {
        // Early out to avoid an unneeded `align_to` call.
        if data.len() != size_of::<Self>() {
            return None;
        }

        // Safe because the DataInit trait asserts any data is valid for this type, and we ensured
        // the size of the pointer's buffer is the correct size. The `align_to` method ensures that
        // we don't have any unaligned references. This aliases a pointer, but because the pointer
        // is from a const slice reference, there are no mutable aliases. Finally, the reference
        // returned can not outlive data because they have equal implicit lifetime constraints.
        match unsafe { data.align_to::<Self>() } {
            ([], [mid], []) => Some(mid),
            _ => None,
        }
    }

    /// Converts a mutable slice of raw data into a mutable reference of `Self`.
    ///
    /// Because `Self` is made from a reference to the mutable slice`, mutations to the returned
    /// reference are immediately reflected in `data`. The value of the returned `Self` will depend
    /// on the representation of the type in memory, and may change in an unstable fashion.
    ///
    /// This will return `None` if the length of data does not match the size of `Self`, or if the
    /// data is not aligned for the type of `Self`.
    fn from_mut_slice(data: &mut [u8]) -> Option<&mut Self> {
        // Early out to avoid an unneeded `align_to_mut` call.
        if data.len() != size_of::<Self>() {
            return None;
        }

        // Safe because the DataInit trait asserts any data is valid for this type, and we ensured
        // the size of the pointer's buffer is the correct size. The `align_to` method ensures that
        // we don't have any unaligned references. This aliases a pointer, but because the pointer
        // is from a mut slice reference, we borrow the passed in mutable reference. Finally, the
        // reference returned can not outlive data because they have equal implicit lifetime
        // constraints.
        match unsafe { data.align_to_mut::<Self>() } {
            ([], [mid], []) => Some(mid),
            _ => None,
        }
    }

    /// Converts a reference to `self` into a slice of bytes.
    ///
    /// The value of `self` is not copied. Instead, the slice is made from a reference to `self`.
    /// The value of bytes in the returned slice will depend on the representation of the type in
    /// memory, and may change in an unstable fashion.
    fn as_slice(&self) -> &[u8] {
        // Safe because the entire size of self is accessible as bytes because the trait guarantees
        // it. The lifetime of the returned slice is the same as the passed reference, so that no
        // dangling pointers will result from this pointer alias.
        unsafe { from_raw_parts(self as *const Self as *const u8, size_of::<Self>()) }
    }

    /// Converts a mutable reference to `self` into a mutable slice of bytes.
    ///
    /// Because the slice is made from a reference to `self`, mutations to the returned slice are
    /// immediately reflected in `self`. The value of bytes in the returned slice will depend on
    /// the representation of the type in memory, and may change in an unstable fashion.
    fn as_mut_slice(&mut self) -> &mut [u8] {
        // Safe because the entire size of self is accessible as bytes because the trait guarantees
        // it. The trait also guarantees that any combination of bytes is valid for this type, so
        // modifying them in the form of a byte slice is valid. The lifetime of the returned slice
        // is the same as the passed reference, so that no dangling pointers will result from this
        // pointer alias. Although this does alias a mutable pointer, we do so by exclusively
        // borrowing the given mutable reference.
        unsafe { from_raw_parts_mut(self as *mut Self as *mut u8, size_of::<Self>()) }
    }
}

/// A container to host byte and access its content.
///
/// Candidates implement this trait include:
/// - anonymous memory areas
/// - mmapped memory areas
/// - data files
/// - a proxy to access memory on remote
pub trait Bytes<A> {
    /// Associated error codes
    type E;

    /// Writes a slice to the region at the specified address.
    /// Returns the number of bytes written. The number of bytes written can
    /// be less than the length of the slice if there isn't enough room in the
    /// region.
    fn write(&self, buf: &[u8], addr: A) -> Result<usize, Self::E>;

    /// Reads to a slice from the region at the specified address.
    /// Returns the number of bytes read. The number of bytes read can be less than the length
    /// of the slice if there isn't enough room in the region.
    fn read(&self, buf: &mut [u8], addr: A) -> Result<usize, Self::E>;

    /// Writes the entire contents of a slice to the region at the specified address.
    ///
    /// Returns an error if there isn't enough room in the region to complete the entire write.
    /// Part of the data may have been written nevertheless.
    fn write_slice(&self, buf: &[u8], addr: A) -> Result<(), Self::E>;

    /// Reads from the regionbat the specified address to fill the entire buffer.
    ///
    /// Returns an error if there isn't enough room in the region to fill the entire buffer.
    /// Part of the buffer may have been filled nevertheless.
    fn read_slice(&self, buf: &mut [u8], addr: A) -> Result<(), Self::E>;

    /// Writes an object to the region at the specified address.
    /// Returns Ok(()) if the object fits, or Err if it extends past the end.
    fn write_obj<T: DataInit>(&self, val: T, addr: A) -> Result<(), Self::E>;

    /// Reads an object from the region at the given address.
    /// Reading from a volatile area isn't strictly safe as it could change mid-read.
    /// However, as long as the type T is plain old data and can handle random initialization,
    /// everything will be OK.
    fn read_obj<T: DataInit>(&self, addr: A) -> Result<T, Self::E>;

    /// Writes data from a readable object like a File and writes it to the region.
    ///
    /// # Arguments
    /// * `addr` - Begin writing at this address.
    /// * `src` - Read from `src` to the region.
    /// * `count` - Read `count` bytes from `src` to the region.
    fn write_from_stream<F>(&self, addr: A, src: &mut F, count: usize) -> Result<(), Self::E>
    where
        F: Read;

    /// Reads data from the region to a writable object.
    ///
    /// # Arguments
    /// * `addr` - Begin reading from this addr.
    /// * `dst` - Write from the region to `dst`.
    /// * `count` - Read `count` bytes from the region to `dst`.
    fn read_into_stream<F>(&self, addr: A, dst: &mut F, count: usize) -> Result<(), Self::E>
    where
        F: Write;
}

// All intrinsic types and arrays of intrinsic types are DataInit. They are just numbers.
macro_rules! array_data_init {
    ($T:ty, $($N:expr)+) => {
        $(
            unsafe impl DataInit for [$T; $N] {}
        )+
    }
}
macro_rules! data_init_type {
    ($T:ty) => {
        unsafe impl DataInit for $T {}
        array_data_init! {
            $T,
            0  1  2  3  4  5  6  7  8  9
            10 11 12 13 14 15 16 17 18 19
            20 21 22 23 24 25 26 27 28 29
            30 31 32
        }
    };
}
data_init_type!(u8);
data_init_type!(u16);
data_init_type!(u32);
data_init_type!(u64);
data_init_type!(usize);
data_init_type!(i8);
data_init_type!(i16);
data_init_type!(i32);
data_init_type!(i64);
data_init_type!(isize);

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::mem::{align_of, size_of};
    use DataInit;

    fn from_slice_alignment<T>()
    where
        T: DataInit + PartialEq + Debug + Default,
    {
        let mut v = [0u8; 32];
        let pre_len = {
            let (pre, _, _) = unsafe { v.align_to::<T>() };
            pre.len()
        };
        {
            let aligned_v = &mut v[pre_len..pre_len + size_of::<T>()];
            {
                let from_aligned = T::from_slice(aligned_v);
                let val: T = Default::default();
                assert_eq!(from_aligned, Some(&val));
            }
            {
                let from_aligned_mut = T::from_mut_slice(aligned_v);
                let mut val: T = Default::default();
                assert_eq!(from_aligned_mut, Some(&mut val));
            }
        }
        for i in 1..size_of::<T>() {
            let begin = pre_len + i;
            let end = begin + size_of::<T>();
            let unaligned_v = &mut v[begin..end];
            {
                let from_unaligned = T::from_slice(unaligned_v);
                if align_of::<T>() != 1 {
                    assert_eq!(from_unaligned, None);
                }
            }
            {
                let from_unaligned_mut = T::from_mut_slice(unaligned_v);
                if align_of::<T>() != 1 {
                    assert_eq!(from_unaligned_mut, None);
                }
            }
        }
    }

    #[test]
    fn test_slice_alignment() {
        from_slice_alignment::<u8>();
        from_slice_alignment::<u16>();
        from_slice_alignment::<u32>();
        from_slice_alignment::<u64>();
        from_slice_alignment::<usize>();
        from_slice_alignment::<i8>();
        from_slice_alignment::<i16>();
        from_slice_alignment::<i32>();
        from_slice_alignment::<i64>();
        from_slice_alignment::<isize>();
    }
}
