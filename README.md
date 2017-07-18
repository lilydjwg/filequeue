filequeue: send logs to Kafka, using the log file like a queue, without rotation.

What does it do?
====

filequeue sends your line-based logs to Apache Kafka without loss during log
rotation, because you don't need to rotate your log files any more.

filequeue truncates the log file from beginning as it reads, to keep the log
file at a reasonable size.

Requirements
====

* The log file should be line-based: every line is a message to be sent to Kafka
* The log file should be in valid UTF-8 encoding
* Linux >= 3.15 is required because it uses the `FALLOC_FL_COLLAPSE_RANGE` mode of fallocate(2)
* The log file should be on XFS (or ext4 with a recent kernel; 3.16 may not work correctly).

Building
====

[Install rust and cargo](https://www.rust-lang.org/install.html). You can optionally install librdkafka1 >= 0.9.5. If you don't, one will be built and statically linked into the binary. For Arch Linux users, you should install librdkafka1 because its libraries doesn't support static linking.

Run `cargo build --release` to build. You can find the binary at `target/release/filequeue`.

Running
====

Copy and edit `config.yaml` to suit your needs. Then pass the file path to `filequeue` as the sole argument. filequeue doesn't daemonize itself, please use a service manager like [supervisord](http://supervisord.org/) or systemd.
