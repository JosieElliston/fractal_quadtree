use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

use eframe::egui::Color32;

use crate::{complex::fixed::*, sample};

pub(crate) static ELAPSED_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Worker {
    handle: thread::JoinHandle<()>,
    request_sender: mpsc::Sender<(Real, Imag)>,
    in_flight: usize,
}
pub(crate) struct Pool {
    workers: Vec<Worker>,
    response_receiver: mpsc::Receiver<(usize, (Real, Imag), Color32)>,
}
impl Default for Pool {
    fn default() -> Self {
        // const THREAD_COUNT: usize = 8;
        // const THREAD_COUNT: usize = 32;
        // Self::new(THREAD_COUNT)

        // leave one thread for main and other processes,
        // but still take at least one thread
        let thread_count = (thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1)
            - 1)
        .max(1);
        Self::new(thread_count)
    }
}
impl Pool {
    pub(crate) fn new(thread_count: usize) -> Self {
        let (response_sender, response_receiver) = mpsc::channel();
        let workers = (0..thread_count)
            .map(|thread_i| {
                let response_sender = response_sender.clone();
                let (request_sender, request_receiver) = mpsc::channel();

                let handle = thread::Builder::new()
                    .name(format!("pool {}", thread_i))
                    .spawn(move || {
                        while let Ok(point) = request_receiver.recv() {
                            let start = std::time::Instant::now();
                            let color = sample::metabrot_sample(point).color();
                            #[cfg(false)]
                            {
                                let elapsed = start.elapsed();
                                ELAPSED_NANOS.fetch_add(
                                    elapsed.as_nanos() as u64,
                                    std::sync::atomic::Ordering::Relaxed,
                                );
                                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                            let Ok(_) = response_sender.send((thread_i, point, color)) else {
                                break;
                            };
                        }
                    })
                    .unwrap();
                Worker {
                    handle,
                    request_sender,
                    in_flight: 0,
                }
            })
            .collect();
        Self {
            workers,
            response_receiver,
        }
    }
    pub(crate) fn in_flight(&self) -> usize {
        self.workers.iter().map(|worker| worker.in_flight).sum()
    }
    pub(crate) fn thread_count(&self) -> usize {
        self.workers.len()
    }

    pub(crate) fn send(&mut self, point: (Real, Imag)) {
        let worker = self
            .workers
            .iter_mut()
            .min_by_key(|worker| worker.in_flight)
            .unwrap();
        worker.request_sender.send(point).unwrap();
        worker.in_flight += 1;
    }

    pub(crate) fn recv(&mut self) -> Option<((Real, Imag), Color32)> {
        if let Ok((thread_i, point, color)) = self.response_receiver.try_recv() {
            assert!(
                self.workers[thread_i].in_flight > 0,
                "this is an invariant of the type"
            );
            self.workers[thread_i].in_flight -= 1;
            Some((point, color))
        } else {
            None
        }
    }
}
