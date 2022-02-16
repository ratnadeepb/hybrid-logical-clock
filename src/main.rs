mod hlc;
mod syncer;
mod timer;

use actix_web::{web, App, HttpRequest, HttpServer, Responder};
use async_std::channel::{bounded, Receiver, Sender, TrySendError};
use hlc::Hlc;
use std::{io, sync::Arc, thread};
use syncer::{Service, SyncMgr};

#[derive(Clone)]
struct Comm {
    sender: Sender<Service>,
    receiver: Receiver<Service>,
    mgr: SyncMgr,
    clock: Arc<Hlc>,
}

impl Comm {
    pub(crate) fn new(
        sender: Sender<Service>,
        receiver: Receiver<Service>,
        mgr: SyncMgr,
        clock: Arc<Hlc>,
    ) -> Self {
        // never want to send/receive more than a single update
        Self {
            sender,
            receiver,
            mgr,
            clock: clock,
        }
    }
}

async fn helper(req: HttpRequest, data: web::Data<Comm>) -> impl Responder {
    // let th = thread::current();
    // println!("Thread: {:#?}: {:#?}", th.name(), th.id());

    let uri = req.uri().to_string();
    let name = uri.split("/").nth(1).unwrap();

    let ts = data.clock.timestamp();

    // let mut service = Service::new("testapp-svc-2");
    // TODO: ask the sync manager for all service backends
    let mut service = Service::new(name);
    service.set_version(ts);

    match data.sender.try_send(service) {
        Ok(_) => {}, //println!("Msg sent successfully"),
        Err(e) => match e {
            TrySendError::Full(_s) => println!("Error channel full"),
            TrySendError::Closed(_s) => println!("Error channel closed"),
        },
    }

    format!("Hi: {:#?}\n", ts)
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    println!("Starting server");

    // // build demo service channels
    let (snd_2_mgr, rcv_frm_actor) = bounded(40); // 40 = num_cpus
    // threads will need channels too
    let (snd_2_actor, rcv_frm_mgr) = bounded(40);

    let sync_mgr = SyncMgr::new(snd_2_actor, rcv_frm_actor);
    let th_sync_mgr = sync_mgr.clone();
    // start sync mgr
    thread::spawn(move || {
        sync_mgr.clone().run();
    });

    // build clock
    let clock = Arc::new(Hlc::new());

    // build the top level Comm
    let comm = Comm::new(snd_2_mgr, rcv_frm_mgr, th_sync_mgr, clock);

    // start the http server
    HttpServer::new(move || {
        let comm_clone = comm.clone();

        App::new()
            .default_service(web::route().to(helper))
            // .route("/", web::get().to(helper))
            .data(comm_clone)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
