use async_std::channel;
use crossbeam::atomic::AtomicCell;
use futures::{executor::LocalPool, task::LocalSpawn};
use std::{cmp::Ordering, sync::Arc, time::Duration};

#[derive(Clone)]
pub(crate) struct SysTime {
    recent: Arc<AtomicCell<(i64, i64)>>,
    period: Duration,
}

impl SysTime {
    fn monotonic() -> (i64, i64) {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };

        let ret = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_COARSE, &mut time) };
        assert_eq!(ret, 0);
        (time.tv_sec, time.tv_nsec)
    }

    pub(crate) fn new(period: Duration) -> Self {
        Self {
            recent: Arc::new(AtomicCell::new(Self::monotonic())),
            period,
        }
    }

    pub(crate) fn load(&self) -> (i64, i64) {
        self.recent.load()
    }

    pub(crate) fn start(&self, recvr: channel::Receiver<bool>) {
        let recent = self.recent.clone();
        let period = self.period;
        let fut = async move {
            println!("Starting the loop"); // debug
            loop {
                // break from the loop if we receive anything on the channel
                // or if the send channel is closed (this part ain't working)
                // otherwise keep updating time at intervals
                match recvr.try_recv() {
                    Ok(_) => {
                        println!("exiting on sender value");
                        break;
                    }
                    Err(e) => match e {
                        channel::TryRecvError::Empty => {
                            std::thread::sleep(period);
                            let time = Self::monotonic();
                            recent.store(time);
                        }
                        channel::TryRecvError::Closed => {
                            println!("exiting on sender drop");
                            break;
                        }
                    },
                }
            }
        };

        let mut pool = LocalPool::new();
        if let Ok(_) = pool.spawner().spawn_local_obj(Box::pin(fut).into()) {
            pool.run();
        }
    }
}

impl PartialEq for SysTime {
    fn eq(&self, other: &Self) -> bool {
        self.load() == other.load()
    }
}

impl PartialOrd for SysTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let localtime = self.load();
        let othertime = other.load();
        Some(localtime.cmp(&othertime))
    }
}

impl Eq for SysTime {}

impl Ord for SysTime {
    fn cmp(&self, other: &Self) -> Ordering {
        let localtime = self.load();
        let othertime = other.load();
        localtime.cmp(&othertime)
    }
}
