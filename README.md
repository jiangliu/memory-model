# memory-model
A library to manage and access virtual machine address space.

The library provides following data structures to manage and access virtual machine address space:
- AddressSpace: abstraction of the virtual machine address space
- GuestMemory: interfaces to access content in virtual machine address space
- GuestAddress: an address in the virtual machine address space
- MemoryMapping: mechanism to map partial or full virtual machine address space into current process
- VolatileMemory: interfaces to volatile access to memory

The library is derived from two upstream projects:
- [crosvm project](https://chromium.googlesource.com/chromiumos/platform/crosvm/) commit 186eb8b0db644892e8ffba8344efe3492bb2b823
- [firecracker project](https://firecracker-microvm.github.io/) commit 80128ea61b305a27df1f751d70415b04b503eae7

# Usage
First, add the following to your `Cargo.toml`:
```toml
memory-model = "0.1"
```
Next, add this to your crate root:
```rust
extern crate memory-model;
```

# TODO List
- import endian.rs from crosvm project
- rebase address\_space.rs
- better documentation and test cases
