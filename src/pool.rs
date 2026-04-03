use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

use eframe::egui::Color32;

use crate::{complex::fixed::*, sample};

struct Worker {
    handle: thread::JoinHandle<()>,
    request_sender: mpsc::Sender<(Real, Imag)>,
    in_flight: usize,
}
pub(crate) struct Pool {
    workers: Vec<Worker>,
    response_receiver: mpsc::Receiver<(usize, (Real, Imag), Color32)>,
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
                            let color = sample::metabrot_sample(point).color();
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
