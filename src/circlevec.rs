use std::{
    sync::{Arc, Mutex},
    vec::Vec,
};

pub struct CircleVec<T> {
    vec: Mutex<Vec<T>>,
    capacity: usize,
}

impl<T: Clone + Default> CircleVec<T> {
    pub fn new(capacity: usize) -> Arc<Self> {
        let mut inner = Vec::<T>::with_capacity(capacity);
        inner.resize_with(capacity, Default::default);
        Arc::new(Self {
            vec: Mutex::new(inner),
            capacity,
        })
    }

    pub fn add(&self, value: T) {
        let mut v = self.vec.lock().unwrap();
        if v.len() >= self.capacity {
            v.remove(0);
        }
        v.push(value);
    }

    pub fn read(&self) -> Vec<T> {
        let v = self.vec.lock().unwrap();
        v.clone()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
