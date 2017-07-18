use std;

use rdkafka::error;

error_chain! {
  foreign_links {
    IOError(std::io::Error);
    ChanError(std::sync::mpsc::SendError<Option<String>>);
    KafkaError(error::KafkaError);
    Utf8Error(std::string::FromUtf8Error);
  }

  errors {
    ProducerGone {
      description("the producer has gone unexpectedly")
    }
  }
}
