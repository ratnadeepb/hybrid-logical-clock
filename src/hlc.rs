use async_std::channel;

use std::{cmp::Ordering, sync::RwLock, time::Duration};

use crate::timer::SysTime;

pub(crate) struct Hlc {
    curtime: SysTime,
    pub(crate) timestamp: RwLock<(i64, i64, i64)>,
}

impl Hlc {
    const PERIOD: Duration = Duration::from_secs(1);
    pub(crate) fn new() -> Self {
        let systime = SysTime::new(Self::PERIOD);
        let (sender, receiver) = channel::bounded(1);
        unsafe {
            signal_hook::low_level::register(signal_hook::consts::SIGINT, move || {
                sender.try_send(true).expect("failed to send");
                std::thread::sleep(Duration::from_secs(10));
                println!("dropping sender");
                drop(&sender);
            })
            .expect("failed to register signal hook")
        };

        let th_systime = systime.clone();

        std::thread::spawn(move || systime.start(receiver));

        let time = th_systime.load();

        Self {
            curtime: th_systime,
            timestamp: RwLock::new((time.0, time.1, 0)),
        }
    }

    pub(crate) fn timestamp(&self) -> (i64, i64, i64) {
        let ctime = self.curtime.load();
        let mut ltime = self.timestamp.write().unwrap();
        if ctime == ((*ltime).0, (*ltime).1) {
            (*ltime).2 += 1;
        } else {
            (*ltime).0 = ctime.0;
            (*ltime).1 = ctime.1;
            (*ltime).2 = 0;
        }
        ((*ltime).0, (*ltime).1, (*ltime).2)
    }

    pub(crate) fn update_timestamp(&self, other: &Self) -> Option<(i64, i64, i64)> {
        if self >= other {
            Some(*self.timestamp.read().unwrap())
        } else {
            let ctime = other.timestamp();
            let mut ltime = self.timestamp.write().unwrap();
            (*ltime).0 = ctime.0;
            (*ltime).1 = ctime.1;
            (*ltime).2 = 0;
            Some(((*ltime).0, (*ltime).1, (*ltime).2))
        }
    }
}

impl PartialEq for Hlc {
    fn eq(&self, other: &Self) -> bool {
        let r1 = *self.timestamp.read().unwrap();
        let r2 = *other.timestamp.read().unwrap();
        r1 == r2
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let r1 = *self.timestamp.read().unwrap();
        let r2 = *other.timestamp.read().unwrap();
        Some(r1.cmp(&r2))
    }
}

impl Eq for Hlc {}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        let r1 = *self.timestamp.read().unwrap();
        let r2 = *other.timestamp.read().unwrap();
        r1.cmp(&r2)
    }
}
