//! 播放列表/队列记忆：退出后再打开恢复上次队列与当前曲（进度不记，从头暂停）。
//! 落盘 `data_root/play_session.json`。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::player::{PlayerSnapshot, QueueItem};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaySession {
    pub items: Vec<QueueItem>,
    pub queue_index: Option<usize>,
    /// 当前曲（单文件播放且不在队列时用于恢复）
    pub track: Option<QueueItem>,
}

pub fn session_path() -> PathBuf {
    crate::paths::data_root().join("play_session.json")
}

pub fn load() -> Option<PlaySession> {
    let text = fs::read_to_string(session_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(session: &PlaySession) {
    let path = session_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(text) = serde_json::to_string_pretty(session) else {
        return;
    };
    let _ = fs::write(&path, text);
}

/// 从播放器快照记忆：队列 + 当前曲（不含进度）。
pub fn save_from_snapshot(snap: &PlayerSnapshot) {
    if snap.queue.is_empty() && snap.track.is_none() {
        // 空会话：清掉记忆，避免下次恢复出残影
        let _ = fs::remove_file(session_path());
        return;
    }
    let track = snap.track.as_ref().map(|t| QueueItem {
        path: t.path.clone(),
        title: t.title.clone(),
        duration_ms: t.duration_ms,
    });
    save(&PlaySession {
        items: snap.queue.clone(),
        queue_index: snap.queue_index,
        track,
    });
}
