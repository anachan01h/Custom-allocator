#![no_std]

use core::{mem::size_of, ptr};
use libc::{
    c_void, memcpy, memset, mmap, munmap, sbrk, MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_EXEC,
    PROT_READ, PROT_WRITE,
};

struct Header {
    size: usize,
    is_mmap: usize,
    next: *mut Header,
}

const ALIGN: usize = 8;
const MAX_BYTE: usize = 512;
const INIT_LIST_SIZE: usize = 512;
const ADD_LIST_SIZE: usize = 512;
const NUM_LIST: usize = MAX_BYTE / ALIGN + 1;
const INIT_HEAP_SIZE: usize = NUM_LIST * (INIT_LIST_SIZE + size_of::<Header>());
static mut IS_INIT_MALLOC: bool = false;
static mut FREE_LISTS: [*mut Header; NUM_LIST] = [ptr::null_mut(); (NUM_LIST)];

fn get_align(size: usize) -> usize {
    (size + ALIGN - 1) / ALIGN * ALIGN
}

fn get_header(ptr: *mut ()) -> *mut Header {
    unsafe { ptr.sub(size_of::<Header>()) as *mut Header }
}

fn init_malloc() -> Result<(), *mut ()> {
    unsafe {
        IS_INIT_MALLOC = true;

        let current_ptr = sbrk(0) as *mut ();
        let ret = sbrk((INIT_HEAP_SIZE as isize).try_into().unwrap()) as *mut ();

        if ret != current_ptr {
            return Err(ret);
        }
        let mut p = ret;
        for i in 1..NUM_LIST {
            FREE_LISTS[i] = p as *mut Header;

            let num_header = INIT_LIST_SIZE / (i * ALIGN);
            for j in 0..num_header {
                let header = p as *mut Header;
                let size = i * ALIGN;
                (*header).size = size;
                (*header).is_mmap = 0;
                (*header).next = ptr::null_mut();

                let next_ptr = p.add(size + size_of::<Header>());
                if j != (num_header - 1) {
                    (*header).next = next_ptr as *mut Header;
                } else {
                    (*header).next = ptr::null_mut();
                }

                p = next_ptr;
            }
        }
    }
    Ok(())
}

fn add_list(size: usize) -> Result<*mut Header, *mut ()> {
    unsafe {
        let current_ptr = sbrk(0) as *mut ();
        let num_header = ADD_LIST_SIZE / size;
        let ret = sbrk(
            (num_header * (size + size_of::<Header>()))
                .try_into()
                .unwrap(),
        ) as *mut ();

        if ret != current_ptr {
            return Err(ret);
        }

        let mut p = ret;
        for j in 0..num_header {
            let header = p as *mut Header;
            (*header).size = size;
            (*header).is_mmap = 0;
            (*header).next = ptr::null_mut();

            let next_ptr = p.add(size + size_of::<Header>());
            if j != (num_header - 1) {
                (*header).next = next_ptr as *mut Header;
            } else {
                (*header).next = ptr::null_mut();
            }

            p = next_ptr;
        }

        Ok(ret as *mut Header)
    }
}

fn find_chunk(size: usize) -> Result<*mut Header, *mut ()> {
    unsafe {
        let index = size / 8;

        if FREE_LISTS[index] == ptr::null_mut() {
            let new_list_ret = add_list(size);

            match new_list_ret {
                Ok(new_list) => {
                    FREE_LISTS[index] = new_list;
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }

        let header = FREE_LISTS[index];

        FREE_LISTS[index] = (*header).next;

        Ok(header)
    }
}

pub fn malloc(size: usize) -> *mut () {
    unsafe {
        if size == 0 {
            return ptr::null_mut();
        }

        if !IS_INIT_MALLOC {
            if init_malloc().is_err() {
                return ptr::null_mut();
            }
        }

        let size_align = get_align(size);

        if size_align <= MAX_BYTE {
            let header_ret = find_chunk(size_align);
            if header_ret.is_err() {
                return ptr::null_mut();
            }
            let header = header_ret.unwrap();
            return (header as *mut ()).add(size_of::<Header>());
        }

        let mmap_size = size_of::<Header>() + size;

        let p = mmap(
            ptr::null_mut(),
            mmap_size,
            PROT_READ | PROT_WRITE | PROT_EXEC,
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            0,
        );

        if p == MAP_FAILED {
            return ptr::null_mut();
        }

        let header = p as *mut Header;
        (*header).size = mmap_size;
        (*header).is_mmap = 1;

        p.add(size_of::<Header>()) as *mut ()
    }
}

pub fn realloc(p: *mut (), size: usize) -> *mut () {
    unsafe {
        let size_align = get_align(size);
        if p == ptr::null_mut() {
            return malloc(size_align);
        }

        let new_ptr = malloc(size_align);
        let header = get_header(p);

        let memcpy_size = if (*header).size < size_align {
            (*header).size
        } else {
            size_align
        };

        memcpy(new_ptr as *mut c_void, p as *const c_void, memcpy_size);

        free(p);
        new_ptr
    }
}

pub fn calloc(number: isize, size: isize) -> *mut () {
    unsafe {
        let total_size = number * size;
        let current_brk = sbrk(0) as *mut ();
        let new_brk = current_brk.offset(total_size);

        if new_brk > (INIT_HEAP_SIZE as *mut ()).offset(INIT_HEAP_SIZE as isize) {
            return ptr::null_mut();
        }

        sbrk(total_size);

        let ptr = current_brk;

        memset(ptr as *mut c_void, 0, total_size as usize);

        ptr
    }
}

pub fn free(p: *mut ()) {
    unsafe {
        if p == ptr::null_mut() {
            return;
        }

        let header = get_header(p);
        let size = (*header).size;
        if (*header).is_mmap == 1 {
            let nummap_ret = munmap(p.sub(size_of::<Header>()) as *mut c_void, size);
            debug_assert!(nummap_ret == 0);
            if nummap_ret == 0 {
            } else {
                let message = "fail munmanp\n";
                let buffer = message.as_ptr() as *const c_void;
                let buffer_len = message.len();
                libc::write(1, buffer, buffer_len);
            }
        } else {
            let index = size / ALIGN;
            let first_header = FREE_LISTS[index];
            FREE_LISTS[index] = header;
            (*header).next = first_header;
        }
    }
}
