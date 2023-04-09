use parking_lot::Mutex;
use std::{fmt::Debug, sync::Arc, vec::Vec};

pub struct CircleVec<T, const N: usize> {
    capacity: usize,
    inner_vec: Mutex<InnerVec<T, N>>,
}

pub struct InnerVec<T, const N: usize> {
    vec: [T; N],
    pointer: usize,
}

impl<T: Clone + Default + Copy + Debug, const N: usize> CircleVec<T, N> {
    pub fn new() -> Arc<Self> {
        // let mut inner = Vec::<T>::with_capacity(capacity);
        // inner.resize_with(capacity, Default::default);

        let inner = [Default::default(); N];

        Arc::new(Self {
            capacity: N,
            inner_vec: Mutex::new(InnerVec {
                vec: inner,
                pointer: 0,
            }),
        })
    }

    pub fn add(&self, value: T) {
        let mut inner = self.inner_vec.lock();
        let p = inner.pointer;
        inner.vec[p] = value;
        inner.pointer += 1;
        if inner.pointer >= self.capacity {
            inner.pointer = 0;
        }
    }

    pub fn read(&self) -> Vec<T> {
        let inner = self.inner_vec.lock();
        let mut out: Vec<T> = Vec::with_capacity(self.capacity);
        out.extend_from_slice(&inner.vec[inner.pointer..self.capacity]);
        out.extend_from_slice(&inner.vec[0..inner.pointer]);
        out
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
