use std::num::Wrapping;

/// Trait for event types stored in the radix heap.
///
/// The queue is monotonic: events are dequeued in non-decreasing time order.
/// `time()` returns the cyclic timestamp used for bucket placement.
pub trait HasTime {
    fn time(&self) -> Wrapping<u32>;
    /// Sentinel value representing "no event".
    fn no_event() -> Self;
    fn is_no_event(&self) -> bool;
}

/// 33-bucket monotonic radix-heap priority queue.
///
/// Bucket index for a time `t` is `32 - (t ^ cur_time).leading_zeros()`,
/// so bucket 0 holds events whose time equals `cur_time`, and bucket 32
/// holds the most distant events.
///
/// Invariant: `cur_time` only moves forward (monotonically).
pub struct RadixHeapQueue<E: HasTime> {
    buckets: [Vec<E>; 33],
    pub cur_time: i64,
    num_enqueued: usize,
}

impl<E: HasTime> RadixHeapQueue<E> {
    pub fn new() -> Self {
        RadixHeapQueue {
            buckets: std::array::from_fn(|_| Vec::new()),
            cur_time: 0,
            num_enqueued: 0,
        }
    }

    #[inline]
    fn bucket_for(&self, time: Wrapping<u32>) -> usize {
        let diff = time.0 ^ (self.cur_time as u32);
        if diff == 0 {
            0
        } else {
            (32 - diff.leading_zeros()) as usize
        }
    }

    /// Enqueue an event. Its time must be >= cur_time (monotonic invariant).
    pub fn enqueue(&mut self, event: E) {
        let bucket = self.bucket_for(event.time());
        self.buckets[bucket].push(event);
        self.num_enqueued += 1;
    }

    /// Dequeue the event with the smallest time.
    ///
    /// Returns `E::no_event()` if the queue is empty.
    pub fn dequeue(&mut self) -> E {
        if self.num_enqueued == 0 {
            return E::no_event();
        }

        // Fast path: bucket 0 has events at exactly cur_time.
        if let Some(event) = self.buckets[0].pop() {
            self.num_enqueued -= 1;
            return event;
        }

        // Find the first non-empty bucket.
        let bi = match self.buckets[1..].iter().position(|b| !b.is_empty()) {
            Some(i) => i + 1,
            None => return E::no_event(),
        };

        // Find the minimum time in that bucket and advance cur_time.
        let min_time = self.buckets[bi]
            .iter()
            .map(|e| e.time())
            .min_by_key(|t| t.0)
            .unwrap();
        self.cur_time = min_time.0 as i64;

        // Redistribute all events from this bucket into lower buckets.
        let events: Vec<E> = self.buckets[bi].drain(..).collect();
        for event in events {
            let new_bucket = self.bucket_for(event.time());
            debug_assert!(new_bucket < bi);
            self.buckets[new_bucket].push(event);
        }

        // Now bucket 0 must have at least one event.
        self.num_enqueued -= 1;
        self.buckets[0].pop().unwrap()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.num_enqueued == 0
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.num_enqueued
    }

    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.num_enqueued = 0;
    }

    pub fn reset(&mut self) {
        self.clear();
        self.cur_time = 0;
    }
}

impl<E: HasTime> Default for RadixHeapQueue<E> {
    fn default() -> Self {
        Self::new()
    }
}
