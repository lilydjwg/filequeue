use std::sync::mpsc;
use std::thread;
use std::collections::HashMap;
use std::fs::File;

use clap::{Arg, App};
use serde::Deserialize;

mod errors;
mod kafkasink;
mod filequeue;

#[derive(Deserialize)]
struct FileConfig {
  path: String,
  lower_size: u64,
  upper_size: u64,
}

#[derive(Deserialize)]
struct Config {
  file: FileConfig,
  kafka_topic: String,
  rdkafka: HashMap<String,String>,
}

fn main() {
  // this will be inherited
  let sigset = unsafe {
    let mut sigset = std::mem::MaybeUninit::uninit();
    let ptr = sigset.as_mut_ptr();
    libc::sigemptyset(ptr);
    libc::sigaddset(ptr, libc::SIGINT);
    libc::sigaddset(ptr, libc::SIGTERM);
    sigset.assume_init()
  };
  let ret = unsafe {
    libc::pthread_sigmask(libc::SIG_BLOCK, &sigset, std::ptr::null_mut())
  };
  if ret != 0 {
    panic!("pthread_sigmask failed: {}", std::io::Error::last_os_error());
  }

  let _guard = slog_envlogger::init().unwrap();

  let matches = App::new("filequeue")
    .about("use log files as a queue to send data to Kafka without rotation")
    .version(clap::crate_version!())
    .arg(Arg::with_name("config")
         .help("config file")
         .required(true))
    .arg(Arg::with_name("debug")
         .long("debug")
         .help("debug mode, output to stdout"))
    .get_matches();

  let configfile = matches.value_of("config").unwrap();
  let debug = matches.is_present("debug");

  let config: Config;
  {
    let mut f = File::open(configfile).unwrap();
    config = serde_yaml::from_reader(&mut f).unwrap();
  }

  let destfile = config.file.path;
  let sig = signalbool::SignalBool::new(
    &[signalbool::Signal::SIGINT, signalbool::Signal::SIGTERM],
    signalbool::Flag::Interrupt).unwrap();
  let (sender, recver) = mpsc::channel();

  {
    let mut fq = filequeue::FileQueue::new(
      &destfile,
      config.file.lower_size,
      config.file.upper_size,
    ).unwrap();
    thread::Builder::new().name("file-handler".into()).spawn(move || {
      let ret = unsafe {
        libc::pthread_sigmask(
          libc::SIG_UNBLOCK, &sigset, std::ptr::null_mut())
      };
      if ret != 0 {
        panic!("pthread_sigmask failed: {}",
               std::io::Error::last_os_error());
      }

      fq.run(sender, sig).unwrap();
    }).unwrap();
  }

  if !debug {
    let mut sink = kafkasink::KafkaSink::new(
      config.kafka_topic, config.rdkafka).unwrap();
    sink.run(recver).unwrap();
  } else {
    use std::process;
    for msg in &recver {
      if let Some(msg) = msg {
        print!("Recv: {}", msg);
      } else {
        process::exit(0);
      }
    }
    process::exit(1);
  }
}
