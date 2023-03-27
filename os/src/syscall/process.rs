//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next,
        TaskStatus, current_user_token, get_syscall_count, get_start_time,
        mmap, munmap,
    },
    timer::get_time_us,
    mm,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    // trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn memcpy(dst: &mut [u8], src: *const u8, len: usize) {
    for i in 0..len {
        unsafe {
            dst[i] = src.add(i).read_volatile();
        }
    }
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    // trace!("kernel: sys_get_time");
    let us = get_time_us();
    let sec = us / 1_000_000;
    let usec = us % 1_000_000;
    let tv = TimeVal{sec, usec};

    let buffers = mm::translated_byte_buffer(current_user_token(), ts as *const u8, core::mem::size_of::<TimeVal>());
    let _cnt = 0;
    let mut src = &tv as *const TimeVal as *const u8;

    // copy data to user space
    for buffer in buffers {
        memcpy(buffer, src, buffer.len());
        unsafe {
            src = src.add(buffer.len());
        }
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    let us = get_time_us();
    let st = get_start_time();
    let sec = (us - st) / 1_000_000;
    let usec = (us - st) % 1_000_000;
    let time = (sec & 0xffff) * 1000 + usec / 1000;

    let ti = TaskInfo {
        status: TaskStatus::Running,
        syscall_times: get_syscall_count(),
        time,
    };

    let buffers = mm::translated_byte_buffer(current_user_token(), _ti as *const u8, core::mem::size_of::<TaskInfo>());
    let mut src = &ti as *const TaskInfo as *const u8;

    // copy data to user space
    for buffer in buffers {
        memcpy(buffer, src, buffer.len());
        unsafe {
            src = src.add(buffer.len());
        }
    }

    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap");
    mmap(_start, _len, _port)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap");
    munmap(_start, _len)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
