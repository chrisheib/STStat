use std::{
    sync::{Arc, Mutex},
    vec::Vec,
};

pub struct CircleVec<T> {
    capacity: usize,
    inner_vec: Mutex<InnerVec<T>>,
}

pub struct InnerVec<T> {
    vec: Vec<T>,
    pointer: usize,
}

impl<T: Clone + Default> CircleVec<T> {
    pub fn new(capacity: usize) -> Arc<Self> {
        let mut inner = Vec::<T>::with_capacity(capacity);
        inner.resize_with(capacity, Default::default);
        Arc::new(Self {
            capacity,
            inner_vec: Mutex::new(InnerVec {
                vec: inner,
                pointer: 0,
            }),
        })
    }

    pub fn add(&self, value: T) {
        let mut inner = self.inner_vec.lock().unwrap();
        let p = inner.pointer;
        inner.vec[p] = value;
        inner.pointer += 1;
        if inner.pointer >= self.capacity {
            inner.pointer = 0;
        }
    }

    pub fn read(&self) -> Vec<T> {
        let inner = self.inner_vec.lock().unwrap();
        let mut out: Vec<T> = Vec::with_capacity(self.capacity);
        out.extend_from_slice(&inner.vec[inner.pointer..self.capacity]);
        out.extend_from_slice(&inner.vec[0..inner.pointer]);
        out
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
