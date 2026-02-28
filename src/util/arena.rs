use std::ops::{Index, IndexMut};

/// Simple index-based arena allocator backed by a `Vec` and a free list.
pub struct Arena<T> {
    items: Vec<T>,
    free_list: Vec<u32>,
}

impl<T: Default> Arena<T> {
    pub fn new() -> Self {
        Arena {
            items: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Allocate a slot, returning its index. Reuses freed slots when available.
    pub fn alloc(&mut self) -> u32 {
        if let Some(idx) = self.free_list.pop() {
            self.items[idx as usize] = T::default();
            idx
        } else {
            let idx = self.items.len() as u32;
            self.items.push(T::default());
            idx
        }
    }

    /// Return a slot to the free list for reuse.
    pub fn free(&mut self, idx: u32) {
        self.free_list.push(idx);
    }

    pub fn get(&self, idx: u32) -> &T {
        &self.items[idx as usize]
    }

    pub fn get_mut(&mut self, idx: u32) -> &mut T {
        &mut self.items[idx as usize]
    }

    /// Drop all items and reset the free list.
    pub fn clear(&mut self) {
        self.items.clear();
        self.free_list.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Borrow the underlying items slice (needed for read-only access while mutating other fields).
    pub fn items(&self) -> &[T] {
        &self.items
    }
}

impl<T: Default> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Index<u32> for Arena<T> {
    type Output = T;
    fn index(&self, idx: u32) -> &T {
        &self.items[idx as usize]
    }
}

impl<T> IndexMut<u32> for Arena<T> {
    fn index_mut(&mut self, idx: u32) -> &mut T {
        &mut self.items[idx as usize]
    }
}
