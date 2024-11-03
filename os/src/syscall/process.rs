//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus,
    },
};
use crate::config::PAGE_SIZE;
use crate::mm::{translated_va_to_pa, MapPermission, VirtAddr, PageTable, StepByOne};
use crate::task::{current_user_token, delete_framed_area, get_current_task_status, get_current_task_syscall_time, get_current_task_time, insert_framed_area};
use crate::timer::get_time_us;

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
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let ts = translated_va_to_pa(current_user_token(), ts as usize) as *mut TimeVal;
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        }
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    let ti = translated_va_to_pa(current_user_token(), ti as usize) as *mut TaskInfo;
    unsafe {
        *ti = TaskInfo {
            status: get_current_task_status(),
            syscall_times: get_current_task_syscall_time(),
            time: get_current_task_time(),
        }
    }
    0
}

pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap");
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + len);
    if start_va.page_offset() != 0 || port & !0x7 != 0 || port & 0x7 == 0 {
        return -1
    }
    let pt = PageTable::from_token(current_user_token());
    let mut start_vpn = start_va.floor();
    for _ in 0..((len + PAGE_SIZE - 1) / PAGE_SIZE) {
        match pt.translate(start_vpn) {
            Some(pte) => {
                if pte.is_valid() {
                    return -1
                }
            }
            None => {}
        }
        start_vpn.step()
    }
    let permission = MapPermission::from_bits_truncate((port << 1) as u8);
    insert_framed_area(start_va, end_va, permission | MapPermission::U);
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + len);
    if start_va.page_offset() != 0 {
        return -1
    }
    let pt = PageTable::from_token(current_user_token());
    let mut start_vpn = start_va.floor();
    for _ in 0..((len + PAGE_SIZE - 1) / PAGE_SIZE) {
        match pt.translate(start_vpn) {
            Some(pte) => {
                if !pte.is_valid() {
                    return -1
                }
            }
            None => return -1
        }
        start_vpn.step()
    }
    delete_framed_area(start_va, end_va);
    0
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
