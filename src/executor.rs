use core::{
    pin::Pin,
    task::{Context, RawWaker, RawWakerVTable, Waker},
};

use cortex_m::asm;
use defmt::{error, info};
use heapless::mpmc::Queue;

pub struct Executor {}

type TaskQueue = Queue<usize, 4>;

static TASK_ID_READY: TaskQueue = TaskQueue::new();

impl Executor {
    pub fn run_tasks(tasks: &mut [Pin<&mut dyn Future<Output = ()>>]) -> ! {
        for task_id in 0..tasks.len() {
            TASK_ID_READY.enqueue(task_id).expect("Task queue is full");
        }
        loop {
            while let Some(task) = TASK_ID_READY.dequeue() {
                if task > tasks.len() {
                    error!("Bad task ID {}", task);
                    continue;
                }
                info!("Running task {}", task);
                let _ = tasks[task]
                    .as_mut()
                    .poll(&mut Context::from_waker(&WakerManager::get_waker(task)));
            }
            asm::wfi();
        }
    }

    // When an interrupt is fired, this method can be called to make sure the appropriate task ID
    // is ran on next poll of the executor
    pub fn wake_task(task_id: usize) {
        info!("Waking task {}", task_id);
        if TASK_ID_READY.enqueue(task_id).is_err() {
            panic!("Task Queue is full");
        }
    }
}

pub trait ExtWaker {
    fn task_id(&self) -> usize;
}

impl ExtWaker for Waker {
    fn task_id(&self) -> usize {
        self.data() as usize
    }
}

pub struct WakerManager {}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    WakerManager::clone,
    WakerManager::wake,
    WakerManager::wake_by_ref,
    WakerManager::drop,
);

impl WakerManager {
    pub fn get_waker(task_id: usize) -> Waker {
        unsafe { Waker::new(task_id as *const (), &VTABLE) }
    }

    unsafe fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }
    unsafe fn wake(p: *const ()) {
        Executor::wake_task(p as usize);
    }
    unsafe fn wake_by_ref(p: *const ()) {
        Executor::wake_task(p as usize);
    }
    unsafe fn drop(_p: *const ()) {}
}
