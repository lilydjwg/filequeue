use std::os::unix::io::AsRawFd;
use std::io::{self, Seek};
use std::fs::{self, File};
use std::path::Path;
use std::ffi::OsString;
use std::os::linux::fs::MetadataExt;
use std::sync::mpsc;
use std::io::{BufReader, BufRead};
use std::mem::swap;

use libc;
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use inotify::{self, watch_mask};
use signalbool;

use errors::*;

const FALLOC_FL_COLLAPSE_RANGE: libc::c_int = 0x08;

fn truncate_at_front<F: AsRawFd>(fd: &F, len: u64)
  -> io::Result<()> {
  let ret = unsafe {
    libc::fallocate(
      fd.as_raw_fd(), FALLOC_FL_COLLAPSE_RANGE, 0, len as libc::off_t)
  };

  if ret == 0 {
    Ok(())
  } else {
    Err(io::Error::last_os_error())
  }
}

pub struct FileQueue {
  reader: BufReader<File>,
  filepath: Box<Path>,
  state_path: Box<Path>,
  state_file: Option<File>,
  lower_size: u64,
  upper_size: u64,
  blksize: u64,
  buffer: Vec<u8>,
}

impl FileQueue {
  pub fn new<P: AsRef<Path>>(path: P, lower_size: u64, upper_size: u64)
    -> Result<Self>
  {
    let file = fs::OpenOptions::new().read(true).write(true)
      .open(path.as_ref())?;

    let mut state_file_name: OsString = ".".to_string().into();
    let name = path.as_ref().file_name().unwrap();
    state_file_name.push(name);
    state_file_name.push(".state");
    let state_path = path.as_ref().with_file_name(&state_file_name);

    let filepath = path.as_ref().into();
    let blksize = file.metadata()?.st_blksize();
    let reader = BufReader::new(file);

    let mut fq = FileQueue {
      reader,
      blksize,
      filepath,
      lower_size,
      upper_size,
      state_path: state_path.into(),
      state_file: None,
      buffer: vec![],
    };
    fq.update_state()?;
    Ok(fq)
  }

  fn update_state(&mut self) -> Result<()> {
    if !self.state_path.exists() {
      return Ok(());
    }

    let mut state_file = File::open(&self.state_path)?;
    let pos = state_file.read_u64::<LittleEndian>()?;
    self.reader.seek(io::SeekFrom::Start(pos))?;
    Ok(())
  }

  fn save_state(&mut self) -> Result<()> {
    info!("saving state");
    let pos = self.reader.seek(io::SeekFrom::Current(0))?;
    let mut state_file = if let Some(ref mut f) = self.state_file {
      f.seek(io::SeekFrom::Start(0))?;
      f
    } else {
      let f = fs::OpenOptions::new().write(true).create(true)
        .open(&self.state_path)?;
      self.state_file = Some(f);
      self.state_file.as_ref().unwrap()
    };
    state_file.write_u64::<LittleEndian>(pos)?;
    Ok(())
  }

  fn may_truncate(&mut self) -> Result<()> {
    let size = self.reader.seek(io::SeekFrom::Current(0))?;
    if size > self.upper_size {
      let blksize = self.blksize;
      let blks_to_remove = (size - self.lower_size) / blksize;
      if blks_to_remove > 0 {
        let bytes = blks_to_remove * blksize;
        info!("truncating {} bytes from {}", bytes, self.filepath.display());
        if let Err(e) = truncate_at_front(self.reader.get_mut(), bytes) {
          if e.kind() == io::ErrorKind::InvalidInput {
            // this may happen?
            warn!("EINVAL fallocate");
            return Ok(());
          } else {
            return Err(e)?;
          }
        }
        self.reader.seek(io::SeekFrom::Current(-(bytes as i64)))?;
      }
    }

    Ok(())
  }

  fn process_all(&mut self, chan: &mpsc::Sender<Option<String>>)
    -> Result<()> {
    let mut read_something = false;
    let mut buf = vec![];
    swap(&mut self.buffer, &mut buf);
    loop {
      let n = self.reader.read_until(b'\n', &mut buf)?;
      if n == 0 || buf[buf.len()-1] != b'\n' {
        debug_assert!(buf.iter().find(|&x| *x == b'\n').is_none());
        self.buffer = buf;
        break;
      }
      read_something = true;
      let line = String::from_utf8(buf)?;
      debug_assert_eq!(line.find('\n').unwrap(), line.len()-1);
      buf = vec![];
      chan.send(Some(line))?;
    }
    if read_something {
      self.may_truncate()?;
      self.save_state()?;
    }
    Ok(())
  }

  /// start tailing the file, return on error
  ///
  /// If the file is truncated, first n bytes data is lost, where n is the
  /// file length before truncating.
  pub fn run(&mut self,
             chan: mpsc::Sender<Option<String>>,
             sig: signalbool::SignalBool,
            ) -> Result<()>{
    let mut notify = inotify::Inotify::init()?;
    // CLOSE_WRITE is for `touch`
    let _desc = notify.add_watch(&self.filepath, watch_mask::MODIFY)?;
    self.process_all(&chan)?;
    let mut buf = vec![0; 1024];
    while !sig.caught() {
      if let Err(e) = notify.read_events_blocking(&mut buf) {
        if e.kind() == io::ErrorKind::Interrupted {
          continue;
        } else {
          return Err(e)?;
        }
      };
      self.process_all(&chan)?;
    }
    self.save_state()?;
    chan.send(None)?;
    Ok(())
  }
}
