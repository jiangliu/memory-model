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
- Decide whether to import endian.rs from crosvm project.
- Better documentation and more test cases.
- Change AddressSpace and GuestAddress to use u64 instead of usize to support following usage case:
On 64-bit arm devices, we usually run a 32-bit userspace with a 64-bit kernel. In this case, the machine word size (usize) that crosvm is compiled with (32-bit) isn't the same as the one the guest kernel, host kernel, hardware is using (64-bit). We used u64 to ensure that the size was always at least as big as needed.
