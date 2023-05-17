use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task, enable_deadlock_detect};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec::Vec;
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    let mutex_id: usize = if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        for tid in 0..process_inner.tasks.len() {
            let task = process_inner.get_task(tid);
            task.inner_exclusive_access().mutex_allocation[id] = 0;
            task.inner_exclusive_access().mutex_need[id] = 0;
        }
        id
    } else {
        process_inner.mutex_list.push(None);
        process_inner.mutex_available.push(0);

        for tid in 0..process_inner.tasks.len() {
            let task = process_inner.get_task(tid);
            task.inner_exclusive_access().mutex_allocation.push(0);
            task.inner_exclusive_access().mutex_need.push(0);
        }

        process_inner.mutex_list.len() - 1
    };

    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new(mutex_id)))
    } else {
        Some(Arc::new(MutexBlocking::new(mutex_id)))
    };

    process_inner.mutex_list[mutex_id] = mutex;
    process_inner.mutex_available[mutex_id] = 1;

    mutex_id as isize
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let detect = process_inner.deadlock_detect;
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.mutex_need[mutex_id] = 1;

    drop(task_inner);
    drop(process_inner);
    drop(process);
    if detect && !_check_mutex() {
        return -0xdead;
    }
    mutex.lock();
    0
}
/// check mutex
fn _check_mutex() -> bool {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let tasks = &process_inner.tasks;
    let mut work = Vec::new();
    let mut finish = Vec::new();

    for ava in process_inner.mutex_available.iter() {
        work.push(*ava);
    }

    for _ in 0..tasks.len() {
        finish.push(false);
    }

    loop {
        let mut all_finish = true;
        let mut any_exec = false;
        for tid in 0..tasks.len() {
            if finish[tid] {
                continue;
            }

            all_finish = false;

            let mut can_run: bool = true;
            let task = process_inner.get_task(tid);
            let task_inner = task.inner_exclusive_access();

            for mutex_id in 0..process_inner.mutex_list.len() {
                if task_inner.mutex_need[mutex_id] > work[mutex_id] {
                    can_run = false;
                    break;
                }
            }

            if can_run {
                finish[tid] = true;
                any_exec = true;
                for mutex_id in 0..process_inner.mutex_list.len() {
                    work[mutex_id] += task_inner.mutex_allocation[mutex_id];
                }
            }

            drop(task_inner);
        }

        if all_finish {
            return true;
        }
        if !any_exec {
            return false;
        }
    }

}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        process_inner.semaphore_available[id] = res_count;

        for tid in 0..process_inner.tasks.len() {
            let task = process_inner.get_task(tid);
            task.inner_exclusive_access().semaphore_allocation[id] = 0;
            task.inner_exclusive_access().semaphore_need[id] = 0;
        }

        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_available.push(res_count);

        for tid in 0..process_inner.tasks.len() {
            let task = process_inner.get_task(tid);
            task.inner_exclusive_access().semaphore_allocation.push(0);
            task.inner_exclusive_access().semaphore_need.push(0);
        }

        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.up(sem_id);
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let detect = process_inner.deadlock_detect;
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());

    // temporary mark for current task
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.semaphore_need[sem_id] += 1;

    drop(task_inner);
    drop(process_inner);

    if detect && !_check_semaphore() {
        return -0xdead;
    }

    // unmark it
    let mut task_inner = task.inner_exclusive_access();
    task_inner.semaphore_need[sem_id] -= 1;
    drop(task_inner);
    
    sem.down(sem_id);
    0
}
/// check semaphore
fn _check_semaphore() -> bool {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let tasks = &process_inner.tasks;
    let mut work = Vec::new();
    let mut finish = Vec::new();

    for ava in process_inner.semaphore_available.iter() {
        work.push(*ava);
    }

    for _ in 0..tasks.len() {
        finish.push(false);
    }

    loop {
        let mut all_finish = true;
        let mut any_exec = false;
        for tid in 0..tasks.len() {
            if finish[tid] {
                continue;
            }

            all_finish = false;

            let mut can_run: bool = true;

            let task = process_inner.get_task(tid);
            let task_inner = task.inner_exclusive_access();

            for sem_id in 0..process_inner.semaphore_list.len() {
                if task_inner.semaphore_need[sem_id] > work[sem_id] {
                    can_run = false;
                    break;
                }
            }
            
            if can_run {
                finish[tid] = true;
                any_exec = true;
                for sem_id in 0..process_inner.semaphore_list.len() {
                    work[sem_id] += task_inner.semaphore_allocation[sem_id];
                }
            }

            drop(task_inner);
        }

        if all_finish {
            return true;
        }
        if !any_exec {
            return false;
        }
    }

}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect");
    if _enabled == 1 {
        enable_deadlock_detect(true)
    } else if _enabled == 0 {
        enable_deadlock_detect(false)
    } else {
        -1
    }
}
