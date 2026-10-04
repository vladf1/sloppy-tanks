//! The network path to one client as the kernel's TCP stack measures it: the lowest
//! round trip and how many data segments had to be sent again (Linux `TCP_INFO`). Every
//! retransmitted segment held up the room messages queued behind it (TCP head-of-line
//! blocking), so these figures show how often players' snapshots stall in transit.
//! Other systems measure nothing, and the monitor then reports zeros.

use std::os::fd::RawFd;

/// One connection's TCP figures since it opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TcpReading {
    /// The lowest round trip the kernel measured over the last few minutes, in
    /// microseconds: the path's latency. The smoothed estimate would also count the
    /// client's delayed acknowledgements, which add up to tens of milliseconds while
    /// traffic flows mostly towards the client.
    pub rtt_us: u32,
    /// Data segments sent to the client, retransmissions included.
    pub data_segments_sent: u64,
    /// Segments sent again because the client did not acknowledge them in time.
    pub retransmitted_segments: u64,
}

impl TcpReading {
    pub fn rtt_ms(&self) -> f64 {
        f64::from(self.rtt_us) / 1000.0
    }

    pub fn retransmit_percent(&self) -> f64 {
        percent(self.retransmitted_segments, self.data_segments_sent)
    }
}

/// `part` as a percentage of `whole`; zero when nothing was sent.
pub fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

/// Reads the TCP socket `fd`. The caller keeps it open, so the figures are that
/// connection's. `None` for anything but a TCP socket, or when the kernel predates the
/// segment counters (Linux 4.6).
#[cfg(target_os = "linux")]
pub fn read(fd: RawFd) -> Option<TcpReading> {
    // SAFETY: getsockopt writes at most `length` bytes into the zeroed struct and
    // reports how many it wrote; an invalid descriptor only makes it fail.
    let mut info: libc::tcp_info = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::tcp_info>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_INFO,
            (&raw mut info).cast(),
            &mut length,
        )
    };
    let needed =
        std::mem::offset_of!(libc::tcp_info, tcpi_data_segs_out) + std::mem::size_of::<u32>();
    if result != 0 || (length as usize) < needed {
        return None;
    }
    Some(TcpReading {
        rtt_us: info.tcpi_min_rtt,
        data_segments_sent: u64::from(info.tcpi_data_segs_out),
        retransmitted_segments: u64::from(info.tcpi_total_retrans),
    })
}

#[cfg(not(target_os = "linux"))]
pub fn read(_fd: RawFd) -> Option<TcpReading> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retransmits_are_a_share_of_the_data_segments_sent() {
        let reading = TcpReading {
            rtt_us: 85_400,
            data_segments_sent: 800,
            retransmitted_segments: 6,
        };
        assert_eq!(reading.rtt_ms(), 85.4);
        assert_eq!(reading.retransmit_percent(), 0.75);
        assert_eq!(TcpReading::default().retransmit_percent(), 0.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_loopback_connection_reports_its_segments() {
        use std::io::{Read, Write};
        use std::os::fd::AsRawFd;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server.write_all(b"snapshot").unwrap();
        let mut received = [0; 8];
        client.read_exact(&mut received).unwrap();
        let reading = read(server.as_raw_fd()).expect("Linux reports TCP_INFO");
        assert!(reading.data_segments_sent >= 1);
        assert_eq!(reading.retransmitted_segments, 0);
        assert!(read(-1).is_none(), "an invalid descriptor reads nothing");
    }
}
