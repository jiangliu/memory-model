// Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.
//
//! The mmap_unix module provides internal unix specific abstractions
//! for the mmap module.

use libc::c_void;
pub use libc::MAP_FAILED;
use std::os::unix::io::AsRawFd;
use std::ptr::null_mut;

pub type RawFile = std::os::unix::io::RawFd;
pub trait AsRawFile {
    fn as_raw_file(&self) -> RawFile;
}

impl AsRawFile for std::fs::File {
    fn as_raw_file(&self) -> RawFile {
        self.as_raw_fd()
    }
}

pub unsafe fn map_anon_mem(size: usize) -> *mut c_void {
    return libc::mmap(
        null_mut(),
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_ANONYMOUS | libc::MAP_SHARED | libc::MAP_NORESERVE,
        -1,
        0,
    );
}

pub unsafe fn map_shared_mem(file: &AsRawFile, size: usize, offset: usize) -> *mut c_void {
    return libc::mmap(
        null_mut(),
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED,
        file.as_raw_file(),
        offset as libc::off_t,
    );
}

pub unsafe fn unmap_mem(addr: *mut c_void, size: usize) -> i32 {
    // madvising away the region is the same as the guest changing it.
    // Next time it is read, it may return zero pages.
    return libc::madvise(addr, size, libc::MADV_REMOVE);
}

pub unsafe fn release_mem(addr: *mut c_void, size: usize) {
    libc::munmap(addr, size);
}
