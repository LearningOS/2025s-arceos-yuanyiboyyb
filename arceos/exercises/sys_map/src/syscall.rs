#![allow(dead_code)]

use core::ffi::{c_void, c_char, c_int};
use axhal::arch::TrapFrame;
use axhal::trap::{register_trap_handler, SYSCALL};
use axerrno::LinuxError;
use axtask::{current, current_may_uninit};
use axtask::TaskExtRef;
use axhal::paging::MappingFlags;
use arceos_posix_api as api;
 use axhal::mem::phys_to_virt;
use memory_addr::{
    align_down_4k, is_aligned_4k, pa, va, MemoryAddr, PageIter4K, PhysAddr, VirtAddr, VirtAddrRange, PAGE_SIZE_4K,align_up_4k
};
use arceos_posix_api::{get_file_like};
const SYS_IOCTL: usize = 29;
const SYS_OPENAT: usize = 56;
const SYS_CLOSE: usize = 57;
const SYS_READ: usize = 63;
const SYS_WRITE: usize = 64;
const SYS_WRITEV: usize = 66;
const SYS_EXIT: usize = 93;
const SYS_EXIT_GROUP: usize = 94;
const SYS_SET_TID_ADDRESS: usize = 96;
const SYS_MMAP: usize = 222;

const AT_FDCWD: i32 = -100;

/// Macro to generate syscall body
///
/// It will receive a function which return Result<_, LinuxError> and convert it to
/// the type which is specified by the caller.
#[macro_export]
macro_rules! syscall_body {
    ($fn: ident, $($stmt: tt)*) => {{
        #[allow(clippy::redundant_closure_call)]
        let res = (|| -> axerrno::LinuxResult<_> { $($stmt)* })();
        match res {
            Ok(_) | Err(axerrno::LinuxError::EAGAIN) => debug!(concat!(stringify!($fn), " => {:?}"),  res),
            Err(_) => info!(concat!(stringify!($fn), " => {:?}"), res),
        }
        match res {
            Ok(v) => v as _,
            Err(e) => {
                -e.code() as _
            }
        }
    }};
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// permissions for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapProt: i32 {
        /// Page can be read.
        const PROT_READ = 1 << 0;
        /// Page can be written.
        const PROT_WRITE = 1 << 1;
        /// Page can be executed.
        const PROT_EXEC = 1 << 2;
    }
}

impl From<MmapProt> for MappingFlags {
    fn from(value: MmapProt) -> Self {
        let mut flags = MappingFlags::USER;
        if value.contains(MmapProt::PROT_READ) {
            flags |= MappingFlags::READ;
        }
        if value.contains(MmapProt::PROT_WRITE) {
            flags |= MappingFlags::WRITE;
        }
        if value.contains(MmapProt::PROT_EXEC) {
            flags |= MappingFlags::EXECUTE;
        }
        flags
    }
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// flags for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapFlags: i32 {
        /// Share changes
        const MAP_SHARED = 1 << 0;
        /// Changes private; copy pages on write.
        const MAP_PRIVATE = 1 << 1;
        /// Map address must be exactly as requested, no matter whether it is available.
        const MAP_FIXED = 1 << 4;
        /// Don't use a file.
        const MAP_ANONYMOUS = 1 << 5;
        /// Don't check for reservations.
        const MAP_NORESERVE = 1 << 14;
        /// Allocation is for a stack.
        const MAP_STACK = 0x20000;
    }
}

#[register_trap_handler(SYSCALL)]
fn handle_syscall(tf: &TrapFrame, syscall_num: usize) -> isize {
    ax_println!("handle_syscall [{}] ...", syscall_num);
    let ret = match syscall_num {
        SYS_IOCTL => sys_ioctl(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _) as _,
        SYS_SET_TID_ADDRESS => sys_set_tid_address(tf.arg0() as _),
        SYS_OPENAT => sys_openat(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _, tf.arg3() as _),
        SYS_CLOSE => sys_close(tf.arg0() as _),
        SYS_READ => sys_read(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITE => sys_write(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITEV => sys_writev(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_EXIT_GROUP => {
            ax_println!("[SYS_EXIT_GROUP]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_EXIT => {
            ax_println!("[SYS_EXIT]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_MMAP => sys_mmap(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3() as _,
            tf.arg4() as _,
            tf.arg5() as _,
        ),
        _ => {
            ax_println!("Unimplemented syscall: {}", syscall_num);
            -LinuxError::ENOSYS.code() as _
        }
    };
    ret
}

#[allow(unused_variables)]
fn sys_mmap(
    addr: *mut usize,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    _offset: isize,
) -> isize {
    const MAX_MMAP_SIZE: usize = 64;
    let mut buf: [u8; 64] = [0u8;MAX_MMAP_SIZE];
    unsafe {
        let buf_ptr = &mut buf as *mut _ as *mut c_void;
        sys_read(fd, buf_ptr, length+_offset as usize);
    }
    let mut buf = &buf[_offset as usize..length+_offset as usize];

    let binding = current();
    let mut uspace = &mut binding.task_ext().aspace.lock();

    let free_va = if addr.is_null() {
        uspace.find_free_area(
            (addr as usize).into(), 
            length, 
            VirtAddrRange::new(
                uspace.base(),
                uspace.end()))
            .unwrap_or_else(|| panic!("No free area for mmap"))
    }else{
        (addr as usize).into()
    };

    // 把prot转换成MappingFlags
    let mut flags = MappingFlags::from(MmapProt::from_bits_truncate(prot));
    flags.set(MappingFlags::USER, true);

    uspace.map_alloc(
        free_va, 
        PAGE_SIZE_4K, 
        flags, 
        true)
        .unwrap();
    let (paddr, _, _) = uspace
        .page_table()
        .query(free_va)
        .unwrap_or_else(|_| panic!("Mapping failed for segment"));
    unsafe {
        core::ptr::copy_nonoverlapping(
            buf.as_ptr(),
            phys_to_virt(paddr).as_mut_ptr(),
            PAGE_SIZE_4K,
        );
    }
    free_va.as_usize() as isize
    /* //unimplemented!("no sys_mmap!");
    let flags = match MmapFlags::from_bits(flags) {
        Some(flags) => flags,
        None => return -1, // 直接从 sys_mmap 函数返回
    };
    let prot = match MmapProt::from_bits(prot) {
        Some(prot) => prot,
        None => return -1, // 直接从 sys_mmap 函数返回
    };
    if !flags.contains(MmapFlags::MAP_ANONYMOUS) && fd == 0 {
        return -1; // 直接从 sys_mmap 函数返回
    }
    let mut curr = current_may_uninit().unwrap();
    let aspace = &curr.task_ext().aspace;
    let a = match aspace.lock().map_alloc(VirtAddr::from_mut_ptr_of(addr).align_up_4k(), align_up_4k(length), prot.into(), true){
        Ok(()) =>{
            if fd!=0{
                match get_file_like(fd) {  
                    Ok(file) => {  
                        let buf = unsafe {   
                            core::slice::from_raw_parts_mut(addr as *mut u8, length)   
                        };  
                        match file.read(buf) {  
                            Ok(_bytes_read) =>addr as isize,  
                            Err(_) => -1, 
                        }  
                    }  
                    Err(_) => -1, 
                }  
            }else{
                addr as isize
            }
        }
        Err(_)=> -1
    };
    a */
}

fn sys_openat(dfd: c_int, fname: *const c_char, flags: c_int, mode: api::ctypes::mode_t) -> isize {
    assert_eq!(dfd, AT_FDCWD);
    api::sys_open(fname, flags, mode) as isize
}

fn sys_close(fd: i32) -> isize {
    api::sys_close(fd) as isize
}

fn sys_read(fd: i32, buf: *mut c_void, count: usize) -> isize {
    api::sys_read(fd, buf, count)
}

fn sys_write(fd: i32, buf: *const c_void, count: usize) -> isize {
    api::sys_write(fd, buf, count)
}

fn sys_writev(fd: i32, iov: *const api::ctypes::iovec, iocnt: i32) -> isize {
    unsafe { api::sys_writev(fd, iov, iocnt) }
}

fn sys_set_tid_address(tid_ptd: *const i32) -> isize {
    let curr = current();
    curr.task_ext().set_clear_child_tid(tid_ptd as _);
    curr.id().as_u64() as isize
}

fn sys_ioctl(_fd: i32, _op: usize, _argp: *mut c_void) -> i32 {
    ax_println!("Ignore SYS_IOCTL");
    0
}
