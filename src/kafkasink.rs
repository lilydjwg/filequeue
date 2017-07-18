use std::sync::mpsc;
use std::collections::HashMap;
use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::producer::{BaseProducer, EmptyProducerContext};

use errors::*;

pub struct KafkaSink {
  client: BaseProducer<EmptyProducerContext>,
  topic: String,
}

impl KafkaSink {
  pub fn new<S: Into<String>>(topic: S, confmap: HashMap<String,String>)
    -> Result<Self> {
    let mut config = ClientConfig::new();
    for (k, v) in confmap {
      config.set(&k, &v);
    }

    Ok(KafkaSink {
      client: config.create()?,
      topic: topic.into(),
    })
  }

  pub fn run(&mut self, chan: mpsc::Receiver<Option<String>>)
    -> Result<()> {
    loop {
      match chan.recv_timeout(Duration::from_secs(1)) {
        Ok(Some(msg)) => {
          self.client.send_copy::<_,str>(
            &self.topic, None, Some(&msg), None, None, None)?;
          self.client.poll(0);
        },
        Ok(None) => break,
        Err(mpsc::RecvTimeoutError::Timeout) => {
          self.client.poll(0);
        },
        Err(mpsc::RecvTimeoutError::Disconnected) => {
          return Err(ErrorKind::ProducerGone.into());
        },
      };
    }
    Ok(())
  }
}
