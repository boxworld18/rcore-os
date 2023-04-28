//!Implementation of [`Processor`] and Intersection of control flow
//!
//! Here, the continuous operation of user apps in CPU is maintained,
//! the current running state of CPU is recorded,
//! and the replacement and transfer of control flow of different applications are executed.

use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use crate::config::{MAX_SYSCALL_NUM, BIG_STRIDE};
use crate::mm::{VirtAddr, VirtPageNum, MapPermission};
use alloc::sync::Arc;
use lazy_static::*;

/// Processor management structure
pub struct Processor {
    ///The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,

    ///The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    ///Create an empty Processor
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    ///Get mutable reference to `idle_task_cx`
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    ///Get current task in moving semanteme
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    ///Get current task in cloning semanteme
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

///The main part of process execution and scheduling
///Loop `fetch_task` to get the process that needs to run, and switch the process through `__switch`
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            task_inner.stride += BIG_STRIDE / task_inner.priority;
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            // release coming task_inner manually
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            warn!("no tasks available in run_tasks");
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get the current user token(addr of page table)
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

///Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

///Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}

/// Update syscall count of specific task
#[allow(unused)]
pub fn update_syscall_count(syscall_id: usize) {
    if let Some(task) = current_task() {
        let mut inner = task.inner_exclusive_access();
        inner.syscall_count[syscall_id] += 1;
    }
}

/// Get syscall count of specific task
#[allow(unused)]
pub fn get_syscall_count() -> [u32; MAX_SYSCALL_NUM] {
    if let Some(task) = current_task() {
        let inner = task.inner_exclusive_access();
        return inner.syscall_count
    }
    panic!("unreachable!")
}

/// Get start time of specific task
#[allow(unused)]
pub fn get_start_time() -> usize {
    if let Some(task) = current_task() {
        let inner = task.inner_exclusive_access();
        return inner.start_time
    }
    panic!("unreachable!")
}

/// Memory map
#[allow(unused)]
pub fn mmap(addr: usize, len: usize, port: usize) -> isize {
    if let Some(task) = current_task() {
        if port & !0x7 != 0 || port & 0x7 == 0 {
            return -1;
        }
        // invalid addr
        if addr % 4096 != 0 {
            return -1;
        }

        let mut inner = task.inner_exclusive_access();
        let memset = &mut inner.memory_set;

        let st_va = VirtAddr::from(addr);
        let ed_va = VirtAddr::from(addr + len);

        let _st = st_va.floor();
        let _ed = ed_va.ceil();

        for vpn in _st.0 .._ed.0 {
            trace!{"allocate vpn: {:x}", vpn};
            if let Some(_tmp) = memset.translate(VirtPageNum::from(vpn)) {
                if _tmp.is_valid() {
                    trace!("{:x} already mapped", vpn);
                    return -1;
                }
            }
        }

        let mut per = MapPermission::from_bits_truncate((port as u8) << 1);
        per.set(MapPermission::U, true);
        
        memset.insert_framed_area(st_va, ed_va, per);

        return 0
    }
    panic!("unreachable!")
}

/// Memory unmap
#[allow(unused)]
pub fn munmap(addr: usize, len: usize) -> isize {
    if let Some(task) = current_task() {
        if addr % 4096 != 0 {
            return -1;
        }
        let mut inner = task.inner_exclusive_access();
        let memset = &mut inner.memory_set;

        let st_va = VirtAddr::from(addr);
        let ed_va = VirtAddr::from(addr + len);

        let st = st_va.floor();
        let ed = ed_va.ceil();

        for vpn in st.0 ..ed.0 {
            if let Some(_tmp) = memset.translate(VirtPageNum::from(vpn)) {
                if !_tmp.is_valid() {
                    return -1;
                }
            } else {
                return -1;
            }
        }

        for vpn in st.0 ..ed.0 {
            memset.unmap(VirtPageNum::from(vpn));
        }

        return 0
    }
    panic!("unreachable!")
}
