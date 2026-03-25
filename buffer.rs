use std::collections::VecDeque;
use std::sync::Mutex;
use crate::util::SensorData;

// ─────────────────────────────────────────────────────────────────────────────
// Priority-aware bounded buffer
// ─────────────────────────────────────────────────────────────────────────────

pub enum PushResult {
    /// Incoming item accepted — there was space available.
    Accepted,
    /// Incoming accepted, but a lower-priority item was evicted to make room.
    Evicted(SensorData),
    /// Incoming rejected — it was the lowest priority item present.
    Dropped(SensorData),
}

pub struct BoundedBuffer {
    capacity: usize,
    queue:    Mutex<VecDeque<SensorData>>,
}

impl BoundedBuffer {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, queue: Mutex::new(VecDeque::new()) }
    }

    pub fn push(&self, data: SensorData) -> PushResult {
        let mut q = self.queue.lock().unwrap();
        if q.len() < self.capacity {
            q.push_back(data);
            return PushResult::Accepted;
        }
        // Find the item with the highest priority number (= lowest actual priority)
        let worst_idx = q
            .iter()
            .enumerate()
            .max_by_key(|(_, d)| d.priority)
            .map(|(i, _)| i)
            .unwrap();

        if q[worst_idx].priority > data.priority {
            let evicted = q.remove(worst_idx).unwrap();
            q.push_back(data);
            PushResult::Evicted(evicted)
        } else {
            PushResult::Dropped(data)
        }
    }

    pub fn pop(&self) -> Option<SensorData> {
        self.queue.lock().unwrap().pop_front()
    }

    pub fn fill_ratio(&self) -> f64 {
        let q = self.queue.lock().unwrap();
        q.len() as f64 / self.capacity as f64
    }

    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}