## [0.1.0]

### Changes from the upstream crosvm project
Several user visible changes have been applied when importing code from the [crosvm project](https://chromium.googlesource.com/chromiumos/platform/crosvm/), which includes:
- change memory related data fields from u64 to usize
- change definition of mmap::Error::SystemCallFailed
- introduce MemoryMapping::mark\_dontdump() and don't mark mmapped regions as MADV\_DONTDUMP by default
- change behavior of GuestMemory::{address\_in\_range, checked\_offset}, which detects holes now

### Changes from the upstream firecracker project
Several user visible changes have been applied when importing code from the [firecracker project](https://firecracker-microvm.github.io/), which includes:
- import volatile\_memory from crosvm
- introduce MemoryMapping::{from\_fd\_offset, remove\_range, remove\_range}
- introduce GuestMemory::{memory\_size, remove\_range, write\_all\_at\_addr, read\_exact\_at\_addr}
- rename GuestMemory::write\_slice\_at\_addr as GuestMemroy::write\_at\_addr
- rename GuestMemory::read\_slice\_at\_addr as GuestMemroy::read\_at\_addr
