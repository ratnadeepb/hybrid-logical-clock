use std::time::Duration;

use async_std::channel::{Receiver, Sender, TryRecvError};

// Concerns:
// The sync manager uses channels to talk to the threads
// and the threads use a different set of channels to talk to the sync manager
// I am using bounded channels so that neither party is overwhelmed by the volume of messages
// However, that leads to a deadlock concern where both channels can become full
// leaving each process waiting for the other

#[derive(Clone, Debug)]
pub(crate) struct Backend {
    // pod IP
    ip: String,
    // number of requests outstanding
    reqs: u64,
    // total requests so far
    count: u64,
    // last RTT
    rtt: Duration,
    // weighted avg of RTT --> val := last_val + (cur_val - last_val) / total_so_far
    rtt_mean: Duration,
}

impl Backend {
    pub(crate) fn new(ip: &str) -> Self {
        Self {
            ip: ip.to_owned(),
            reqs: 0,
            count: 0,
            rtt: Duration::default(),
            rtt_mean: Duration::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Service {
    name: String,
    backends: Vec<Backend>,
    version: (i64, i64, i64), // hlc timestamp of latest update
}

impl Default for Service {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            backends: Vec::with_capacity(0),
            version: (0, 0, 0),
        }
    }
}

impl Service {
    fn get_backends(name: &str) -> Vec<Backend> {
        // get endpoints for this service
        let b = reqwest::blocking::get(format!("http://localhost:30000/{}", name))
            .unwrap()
            .text()
            .unwrap();
        let mut tmp = b.split(",").collect::<Vec<&str>>();
        tmp.remove(0);
        let tmp0 = tmp[0].split(":[").collect::<Vec<&str>>();
        if tmp0.len() > 1 {
            tmp[0] = tmp0[1];
        }
        let mut ips = tmp
            .drain(..)
            .map(|ip| ip.split("\"").collect::<Vec<&str>>()[1])
            .collect::<Vec<&str>>();
        // println!("{:#?}", ips);

        ips.drain(..).map(|ip| Backend::new(ip)).collect()
    }

    pub(crate) fn new(name: &str) -> Self {
        let backends = Self::get_backends(name);
        Self {
            name: name.to_owned(),
            backends,
            version: (0, 0, 0),
        }
    }

    pub(crate) fn set_version(&mut self, version: (i64, i64, i64)) {
        self.version = version;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SyncMgr {
    // sender: Sender<Service>,     // send to actors
    receiver: Receiver<Service>, // receive from actors
    services: Vec<Service>,      // list of services
}

impl SyncMgr {
    pub(crate) fn new(_sender: Sender<Service>, receiver: Receiver<Service>) -> Self {
        Self {
            // sender,
            receiver,
            services: Vec::new(),
        }
    }

    pub(crate) fn get_service(&self, name: &str) -> Service {
        for svc in self.services.iter() {
            if name == &svc.name {
                return svc.clone();
            }
        }
        Service::new(name)
    }

    pub(crate) fn run(&mut self) {
        loop {
            match self.receiver.try_recv() {
                Ok(svc) => {
                    // assumption is that there can be at most one duplcate
                    match self
                        .services
                        .iter()
                        .enumerate()
                        .find(|(_, s)| &s.name == &svc.name)
                    {
                        Some((ind, service)) => {
                            if service.version < svc.version {
                                // println!("Replacing service");
                                self.services[ind] = svc.clone();
                                // match self.sender.try_send(svc) {
                                //     Ok(_) => println!("successfully sent new version to clients"),
                                //     Err(e) => match e {
                                //         TrySendError::Full(_) => {
                                //             println!("channel full, next time")
                                //         }
                                //         TrySendError::Closed(_) => {
                                //             println!("channel closed, exiting");
                                //             break;
                                //         }
                                //     },
                                // }
                            }
                        }
                        None => {
                            // thread is asking for info
                            if &svc.name != "" {
                                // add this service
                                // println!("Adding service");
                                self.services.push(svc);
                            }
                            // for svc in self.services.iter() {
                            //     match self.sender.try_send(svc.clone()) {
                            //         Ok(_) => println!("successfully sent svc info to clients"),
                            //         Err(e) => match e {
                            //             TrySendError::Full(_) => {
                            //                 println!("channel full, next time")
                            //             }
                            //             TrySendError::Closed(_) => {
                            //                 println!("channel closed, exiting");
                            //                 break;
                            //             }
                            //         },
                            //     }
                            // }
                        }
                    }
                }
                Err(e) => match e {
                    TryRecvError::Empty => {}
                    TryRecvError::Closed => {
                        println!("channel closed from other side");
                        break;
                    }
                },
            }
        }
    }
}
