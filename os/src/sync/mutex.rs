//! Mutex (spin-like and blocking(sleep))

use super::UPSafeCell;
use crate::task::TaskControlBlock;
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Lock the mutex
    fn lock(&self);
    /// Unlock the mutex
    fn unlock(&self);
}

/// Spinlock Mutex struct
pub struct MutexSpin {
    id: usize,
    locked: UPSafeCell<bool>,
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new(mutex_id: usize) -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
            id: mutex_id,
        }
    }
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self) {
        trace!("kernel: MutexSpin::lock");
        loop {
            let mut locked = self.locked.exclusive_access();
            let task = current_task().unwrap();
            let mut task_inner = task.inner_exclusive_access();
            if *locked {
                drop(locked);
                task_inner.mutex_allocation[self.id] = 0;
                task_inner.mutex_need[self.id] = 1;
                drop(task_inner);
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;

                task_inner.mutex_allocation[self.id] = 1;
                task_inner.mutex_need[self.id] = 0;
                let process = task.process.upgrade().unwrap();
                process.inner_exclusive_access().mutex_available[self.id] = 0;

                return;
            }
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        let mut locked = self.locked.exclusive_access();
        *locked = false;

        let task = current_task().unwrap();
        let mut task_inner = task.inner_exclusive_access();
        task_inner.mutex_allocation[self.id] = 0;

        let process = task.process.upgrade().unwrap();
        process.inner_exclusive_access().mutex_available[self.id] = 1;
    }
}

/// Blocking Mutex struct
pub struct MutexBlocking {
    id: usize,
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new(mutex_id: usize) -> Self {
        trace!("kernel: MutexBlocking::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                })
            },
            id: mutex_id,
        }
    }
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self) {
        trace!("kernel: MutexBlocking::lock");
        let mut mutex_inner = self.inner.exclusive_access();
        let task = current_task().unwrap();
        let mut task_inner = task.inner_exclusive_access();
            
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            task_inner.mutex_allocation[self.id] = 0;
            task_inner.mutex_need[self.id] = 1;
            drop(task_inner);
            block_current_and_run_next();
        } else {
            mutex_inner.locked = true;

            task_inner.mutex_allocation[self.id] = 1;
            task_inner.mutex_need[self.id] = 0;

            let process = task.process.upgrade().unwrap();
            process.inner_exclusive_access().mutex_available[self.id] = 0;
        }
    }

    /// unlock the blocking mutex
    fn unlock(&self) {
        trace!("kernel: MutexBlocking::unlock");
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            let mut task_inner = waking_task.inner_exclusive_access();
            task_inner.mutex_allocation[self.id] = 1;
            task_inner.mutex_need[self.id] = 0;
            drop(task_inner);
            wakeup_task(waking_task);
        } else {
            mutex_inner.locked = false;
            let task = current_task().unwrap();
            task.inner_exclusive_access().mutex_allocation[self.id] = 0;
            let process = task.process.upgrade().unwrap();
            process.inner_exclusive_access().mutex_available[self.id] = 1;
        }
    }
}
