#![no_std]

use core::{alloc::Layout, num};
use core::ptr::NonNull;
use allocator::{BaseAllocator, ByteAllocator, PageAllocator,AllocResult,AllocError};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
 


pub struct EarlyAllocator<const PAGE_SIZE: usize>{
    start:usize,
    end:usize,
    b_pos:usize,
    p_pos:usize,
    count:usize,
}
impl <const PAGE_SIZE: usize>EarlyAllocator<PAGE_SIZE> {
    pub const fn new()->Self{
        Self{
            start:0,
            end:0,
            b_pos:0,
            p_pos:0,
            count:0,
        }
    }
}

impl <const PAGE_SIZE: usize>BaseAllocator for EarlyAllocator<PAGE_SIZE> {
    fn init(&mut self, start: usize, size: usize){
        self.start=start;
        self.end=start+size;
        self.b_pos=start;
        self.p_pos=self.end;
        self.count = 0;
    }
    fn add_memory(&mut self, _start: usize, _size: usize) -> allocator::AllocResult {
        unimplemented!()
    }
}

impl <const PAGE_SIZE: usize>ByteAllocator for EarlyAllocator<PAGE_SIZE> {
    fn alloc(&mut self, layout: Layout) -> AllocResult<NonNull<u8>>{
        let  temp = (self.b_pos + layout.align() - 1) / layout.align() * layout.align();
        if temp+layout.size() < self.p_pos{
            self.b_pos = temp+layout.size();
            if let Some(a) = NonNull::new(temp as *mut u8){
                self.count+=1;
                Ok(a)
            }else{
                Err(AllocError::NoMemory)
            }
        }else{
            Err(AllocError::NoMemory)
        }
    }

    fn dealloc(&mut self, _pos: NonNull<u8>, _layout: Layout){
        self.count-=1;
        if self.count <=0{
            self.b_pos = self.start;
            self.count = 0;
        }
    }

    /// Returns total memory size in bytes.
    fn total_bytes(&self) -> usize{
        self.b_pos-self.start
    }

    /// Returns allocated memory size in bytes.
    fn used_bytes(&self) -> usize{
        self.b_pos-self.start
    }

    /// Returns available memory size in bytes.
    fn available_bytes(&self) -> usize{
        self.p_pos-self.b_pos
    }
}

impl <const PAGE_SIZE: usize>PageAllocator for EarlyAllocator<PAGE_SIZE> {
    const PAGE_SIZE: usize = PAGE_SIZE;
    /// Allocate contiguous memory pages with given count and alignment.
    fn alloc_pages(&mut self, num_pages: usize, _align_pow2: usize) -> AllocResult<usize>{
        let  temp = self.p_pos;
        if temp-num_pages*Self::PAGE_SIZE > self.b_pos{
            self.p_pos = temp-Self::PAGE_SIZE*num_pages;
            Ok(self.p_pos)
        }else{
            Err(AllocError::NoMemory)
        }
    }

    /// Deallocate contiguous memory pages with given position and count.
    fn dealloc_pages(&mut self,_pos: usize, _num_pages: usize){
       unimplemented!()
    }

    /// Returns the total number of memory pages.
    fn total_pages(&self) -> usize{
        (self.end-self.p_pos)/Self::PAGE_SIZE
    }

    /// Returns the number of allocated memory pages.
    fn used_pages(&self) -> usize{
        (self.end-self.start)/Self::PAGE_SIZE
    }

    /// Returns the number of available memory pages.
    fn available_pages(&self) -> usize{
        (self.end-self.p_pos)/Self::PAGE_SIZE
    }
}