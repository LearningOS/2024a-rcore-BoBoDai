use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;

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
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!("kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(), tid);
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let deadlock_detective_enable = process_inner.deadlock_detection_enabled;
    if deadlock_detective_enable {
        process_inner.available.push(1);
        process_inner.allocation.iter_mut().for_each(|h| h.push(0));
        process_inner.need.iter_mut().for_each(|h| h.push(0));
    }
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        let id = process_inner.mutex_list.len() as isize - 1;
        id
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!("kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(), tid);
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let deadlock_detective_enable = process_inner.deadlock_detection_enabled;
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    if deadlock_detective_enable {
        let task_num = process_inner.tasks.len();
        process_inner.need[tid][mutex_id] += 1;
        let mut work = process_inner.available[mutex_id];
        let mut finish = vec![false; task_num];
        for i in 0..task_num {
            finish[i] = process_inner.allocation[i][mutex_id] == 0;
        }
        loop {
            let mut found = false;
            for i in 0..task_num {
                if !finish[i] && process_inner.need[i][mutex_id] <= work {
                    work += process_inner.allocation[i][mutex_id];
                    finish[i] = true;
                    found = true;
                }
            }
            if !found {
                break;
            }
        }
        if finish.iter().any(|x| !x) {
            return -0xdead;
        }
        process_inner.allocation[tid][mutex_id] += 1;
        process_inner.available[tid] -= 1;
        process_inner.need[tid][mutex_id] -= 1;
    }
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
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
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!("kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(), tid);
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let deadlock_detection_enabled = process_inner.deadlock_detection_enabled;
    if deadlock_detection_enabled {
        process_inner.available_sem.push(res_count);
        process_inner.allocation_sem.iter_mut().for_each(|h| h.push(0));
        process_inner.need_sem.iter_mut().for_each(|h| h.push(0));
    }
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!("kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),tid);
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let deadlock_detection_enabled = process_inner.deadlock_detection_enabled;
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    if deadlock_detection_enabled {
        if process_inner.allocation_sem[tid][sem_id] > 0 {
            process_inner.allocation_sem[tid][sem_id] -= 1;
            process_inner.available_sem[sem_id] += 1;
        }
    }
    drop(process_inner);
    sem.up();
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!("kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),tid);
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let deadlock_detection_enabled = process_inner.deadlock_detection_enabled;
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    if deadlock_detection_enabled {
        let task_num = process_inner.tasks.len();
        process_inner.need_sem[tid][sem_id] += 1;
        let mut work = process_inner.available_sem[sem_id];
        let mut finish = vec![false; task_num];
        for i in 0..task_num {
            finish[i] = process_inner.allocation_sem[i][sem_id] == 0;
        }
        loop {
            let mut found = false;
            for i in 0..task_num {
                if !finish[i] && process_inner.need_sem[i][sem_id] <= work {
                    work += process_inner.allocation_sem[i][sem_id];
                    finish[i] = true;
                    found = true;
                }
            }
            if !found {
                break;
            }
        }
        if finish.iter().any(|x| !x) || sem_id > 2 {
            return -0xdead;
        }
        if process_inner.available_sem[sem_id] > 0 {
            process_inner.allocation_sem[tid][sem_id] += 1;
            process_inner.available_sem[sem_id] -= 1;
            process_inner.need_sem[tid][sem_id] -= 1;
        }
    }
    drop(process_inner);
    sem.down();
    0
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
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect");
    match enabled {
        0 => current_process().inner_exclusive_access().set_deadlock_detection_enabled(false),
        1 => current_process().inner_exclusive_access().set_deadlock_detection_enabled(true),
        _ => return -1
    }
    0
}
