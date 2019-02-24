# memory-model
A library to manage and access virtual machine's physical memory.

The `memory-model` crate aims to provide a set of stable traits for consumers to access virtual machine's physical memory. Based on these common traits, typical consumers like hypervisors, virtio backend drivers, vhost-user drivers could access guest's physical memory without knowing the implementation details. And thus virtual device backend drivers based on this crate may be reused by different hypervisors.

On the other hand, this crate dosen't define the way how the underline mechanism is implemented to access guest's physical memory. For light-wieght hypervisors like crosvm and firecracker, they may make some assumptions about the structure of virtual machine's physical memory and implement a light-weight backend to access guest's memory. For hypervisors like qemu, a high performance and full functionality backend may be implemented with less assumptions.

This crate is derived from two upstream projects:
- [crosvm project](https://chromium.googlesource.com/chromiumos/platform/crosvm/) commit 186eb8b0db644892e8ffba8344efe3492bb2b823
- [firecracker project](https://firecracker-microvm.github.io/) commit 80128ea61b305a27df1f751d70415b04b503eae7

To be hypervisor neutral, the high level abstraction has been heavily refactored. It could be divided into four parts as:

### Abstraction of Generic Address Space 
Build generic abstractions to describe and access an address space as below:
- AddressValue: stores the raw value of an address. Typically u32, u64 or usize is used to store the raw value. But pointers, such as \*u8, can't be used because it doesn't implement the Add and Sub traits.
- Address: encapsulates an AddressValue object and defines methods to access it.
- AddressRegion: defines methods to access content within an address region. An address region may be continuous or it may have holes within the range [min\_addr, max\_addr) managed by it.
- AddressSpace: extends AddressRegion to build hierarchy architecture. An AnddressSpace object contains a group of non-intersected AddressRegion objects, and the contained AddressRegion object may be another AddressSpace object. By this way, a hierarchy tree may be built to describe an complex address space structure.

To make the abstraction as generic as possible, all the core traits only define methods to access the address space are defined here, and they never define methods to manage (create, delete, insert, remove etc) address spaces. By this way, the address space consumers (virtio device drivers, vhost-user drivers and boot loaders etc) may be decoupled from the address space provider (typically a hypervisor).

### Specialization for Virtual Machine Physical Address Space
The generic address space crates are specialized to access guest's physical memory with following traits:
- GuestAddress: represents a guest physical address (GPA). On ARM64, a 32-bit hypervisor may be used to support a 64-bit guest. For simplicity, u64 is used to store the the raw value no matter the guest a 32-bit or 64-bit virtual machine.
- GuestMemoryRegion: used to represent a continuous region of guest's physical memory.
- GuestMemory: used to represent a collection of GuestMemoryRegion objects. The main responsibilities of the GuestMemory trait are:
	- hide the detail of accessing guest's physical address.
	- map a request address to a GuestMemoryRegion object and relay the request to it.
	- handle cases where an access request spanning two or more GuestMemoryRegion objects.

The virtual machine memory consumers, such as virtio device drivers, vhost drivers and boot loaders etc, should only rely on traits defined here to access guest's memory.

### A Sample and Default Backend Implementation Based on mmap()
Provide a default and sample implementation of the GuestMemory trait by mmapping guest's memory into current process. Three data structures are introduced here:
- MmapRegion: mmap a continous range of guest's physical memory into current and provide methods to access the mmapped memory.
- GuestRegionMmap: a wrapper structure to map guest physical address into (mmap\_region, offset) tuple.
- GuestMemoryMmap: manage a collection of GuestRegionMmap objects for a virtual machine.

One of the main responsibilities of the GuestMemoryMmap object is to handle the use cases where an access request crosses the memory region boundary. This scenario may be triggered when memory hotplug is supported. So there's a tradeoff between functionality code and complexity:
- use following pattern for simplicity which fails when the request crosses region boundary. It's current default behavior in the crosvm and firecracker project.
```rust
	let guest_memory_mmap: GuestMemoryMmap = ...
	let addr: GuestAddress = ...
        let buf = &mut [0u8; 5];
	let result = guest_memory_mmap.find_region(addr).unwrap().write_slice(buf, addr);
```
- use following pattern for functionality to support request crossing region boundary:
```rust
	let guest_memory_mmap: GuestMemoryMmap = ...
	let addr: GuestAddress = ...
        let buf = &mut [0u8; 5];
	let result = guest_memory_mmap.write_slice(buf, addr);
```

### Utilities and Helpers
Following utility and helper traits/macros are imported from the [crosvm project](https://chromium.googlesource.com/chromiumos/platform/crosvm/) with minor changes:
- DataInit: Types for which it is safe to initialize from raw data. A type `T` is `DataInit` if and only if it can be initialized by reading its contents from a byte array. This is generally true for all plain-old-data structs.  It is notably not true for any type that includes a reference.
- {Le,Be}\_{16,32,64}: Explicit endian types useful for embedding in structs or reinterpreting data.
- VolatileMemory: Types for volatile access to memory.

### Relationship among Traits and Structs
- AddressValue
- Address
- AddressRegion
- AddressSpace: AddressRegion
- GuestAddress: Address\<u64\>
- GuestMemoryRegion: AddressRegion<A = GuestAddress, E = Error>
- GuestMemory: AddressSpace<GuestAddress, Error> + AddressRegion<A = GuestAddress, E = Error>
- MmapAddress: Address\<usize\>
- MmapRegion: AddressRegion<A = MmapAddress, E = Error>
- GuestRegionMmap: AddressRegion<A = GuestAddress, E = Error> + GuestMemoryRegion
- GuestMemoryMmap: AddressSpace<GuestAddress, Error> + AddressRegion<A = GuestAddress, E = Error> + GuestMemoryRegion + GuestMemory

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
- Import more interfaces from guest\_memory.rs and mmap.rs
