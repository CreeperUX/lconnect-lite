//! 冷头的 USB 句柄由控制循环（泵、读数、灯圈）和屏幕线程共用，必须加锁。
//! `generation` 在冷头重新打开或推送灯圈后加一：屏幕线程看到它变了就重新初始化并重推画面。

use lcl_devices::h2::H2;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
pub struct H2Slot {
    dev: Mutex<Option<H2>>,
    generation: AtomicU64,
}

impl H2Slot {
    pub fn lock(&self) -> MutexGuard<'_, Option<H2>> {
        self.dev.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// 冷头状态被重置过（重新打开、推送灯圈），屏幕要重新初始化。
    pub fn bump(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}
