// Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.
//
//! The mmap_windows module provides internal windows specific abstractions
//! for the mmap module.
//! 
use libc::{c_void, size_t};
use std;
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::ptr::null;

#[allow(non_snake_case)]
#[link(name = "kernel32")]
extern "stdcall" {
    pub fn VirtualAlloc(
        lpAddress: *mut c_void,
        dwSize: size_t,
        flAllocationType: u32,
        flProtect: u32,
    ) -> *mut c_void;

    pub fn VirtualFree(lpAddress: *mut c_void, dwSize: size_t, dwFreeType: u32) -> u32;

    pub fn CreateFileMappingA(
        hFile: RawHandle,                       // HANDLE
        lpFileMappingAttributes: *const c_void, // LPSECURITY_ATTRIBUTES
        flProtect: u32,                         // DWORD
        dwMaximumSizeHigh: u32,                 // DWORD
        dwMaximumSizeLow: u32,                  // DWORD
        lpName: *const u8,                      // LPCSTR
    ) -> RawHandle; // HANDLE

    pub fn MapViewOfFile(
        hFileMappingObject: RawHandle,
        dwDesiredAccess: u32,
        dwFileOffsetHigh: u32,
        dwFileOffsetLow: u32,
        dwNumberOfBytesToMap: size_t,
    ) -> *mut c_void;

    pub fn CloseHandle(hObject: RawHandle) -> u32; // BOOL
}

pub type RawFile = std::os::windows::io::RawHandle;
pub trait AsRawFile {
    fn as_raw_file(&self) -> RawFile;
}

impl AsRawFile for std::fs::File {
    fn as_raw_file(&self) -> RawFile {
        self.as_raw_handle()
    }
}

const MEM_COMMIT: u32 = 0x00001000;
const MEM_RELEASE: u32 = 0x00008000;
const FILE_MAP_ALL_ACCESS: u32 = 0xf001f;
const PAGE_READWRITE: u32 = 0x04;

pub const MAP_FAILED: *mut c_void = 0 as *mut c_void;
pub const INVALID_HANDLE: RawHandle = (-1isize) as RawHandle;

pub unsafe fn map_anon_mem(size: usize) -> *mut c_void {
    return VirtualAlloc(0 as *mut c_void, size, MEM_COMMIT, PAGE_READWRITE);
}

pub unsafe fn map_shared_mem(file: &AsRawFile, size: usize, offset: usize) -> *mut c_void {
    let handle = file.as_raw_file();
    if handle == INVALID_HANDLE {
        return MAP_FAILED;
    }
    let mapping = CreateFileMappingA(
        handle,
        null(),
        PAGE_READWRITE,
        (size >> 32) as u32,
        size as u32,
        null(),
    );
    if mapping == 0 as RawHandle {
        return MAP_FAILED;
    }
    let view = MapViewOfFile(
        mapping,
        FILE_MAP_ALL_ACCESS,
        (offset >> 32) as u32,
        offset as u32,
        size,
    );
    CloseHandle(mapping);
    return view;
}

pub unsafe fn unmap_mem(addr: *mut c_void, size: usize) -> i32 {
    if VirtualFree(addr, size, MEM_RELEASE) != 0 {
        return 0;
    } else {
        return -1;
    }
}

pub unsafe fn release_mem(addr: *mut c_void, size: usize) {
    VirtualFree(addr, size, MEM_RELEASE);
}
