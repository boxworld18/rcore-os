//! Semaphore

use crate::sync::UPSafeCell;
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self, _sem_id: usize) {
        trace!("kernel: Semaphore::up");
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;

        let cur_task = current_task().unwrap();
        // cur_task.inner_exclusive_access().semaphore_allocation[_sem_id] -= 1;

        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                let mut task_inner = task.as_ref().inner_exclusive_access();
                task_inner.semaphore_allocation[_sem_id] += 1;
                task_inner.semaphore_need[_sem_id] -= 1;
                drop(task_inner);
                wakeup_task(task);
            }
        } else {
            let process = cur_task.process.upgrade().unwrap();
            process.inner_exclusive_access().semaphore_available[_sem_id] += 1;
        }
    }

    /// down operation of semaphore
    pub fn down(&self, _sem_id: usize) {
        trace!("kernel: Semaphore::down");
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;

        let cur_task = current_task().unwrap();
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            cur_task.inner_exclusive_access().semaphore_need[_sem_id] += 1;
            drop(inner);
            block_current_and_run_next();
        } else {
            let process = cur_task.process.upgrade().unwrap();
            process.inner_exclusive_access().semaphore_available[_sem_id] -= 1;
            cur_task.inner_exclusive_access().semaphore_allocation[_sem_id] += 1; 
        }
    }

}
