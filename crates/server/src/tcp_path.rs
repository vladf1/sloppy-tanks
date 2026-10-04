//! The network path to one client as the kernel's TCP stack measures it: the lowest
//! round trip and how many data segments had to be sent again (Linux `TCP_INFO`). Every
//! retransmitted segment held up the room messages queued behind it (TCP head-of-line
//! blocking), so these figures show how often players' snapshots stall in transit.
//! Other systems measure nothing, and the monitor then reports zeros.
//!
//! Behind a reverse proxy the server's own socket only reaches the proxy, so a proxied
//! connection is measured on the proxy's socket to the player instead: the kernel lists
//! every TCP socket on the host to `sock_diag` (what `ss` reads), and the player's
//! address picks theirs out (see [`read_by_peer`]).

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::os::fd::RawFd;

/// One connection's TCP figures since it opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TcpReading {
    /// The lowest round trip the kernel measured over the last few minutes, in
    /// microseconds: the path's latency. The smoothed estimate would also count the
    /// client's delayed acknowledgements, which add up to tens of milliseconds while
    /// traffic flows mostly towards the client. `None` until the kernel has timed a
    /// round trip.
    pub rtt_us: Option<u32>,
    /// Data segments sent to the client, retransmissions included.
    pub data_segments_sent: u64,
    /// Segments sent again because the client did not acknowledge them in time.
    pub retransmitted_segments: u64,
}

impl TcpReading {
    pub fn rtt_ms(&self) -> Option<f64> {
        self.rtt_us.map(|rtt_us| f64::from(rtt_us) / 1000.0)
    }

    pub fn retransmit_percent(&self) -> f64 {
        percent(self.retransmitted_segments, self.data_segments_sent)
    }
}

/// The kernel's lowest round trip before its first sample (`~0U`). The handshake
/// usually gives one, but not when the SYN-ACK was resent to a client without TCP
/// timestamps, so a room socket's first reading can still carry it.
pub const UNMEASURED_RTT_US: u32 = u32::MAX;

/// The kernel's lowest round trip, unless it has not timed one yet.
pub fn measured_rtt(min_rtt_us: u32) -> Option<u32> {
    (min_rtt_us != UNMEASURED_RTT_US).then_some(min_rtt_us)
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
    if result != 0 {
        return None;
    }
    reading_from(&info, length as usize)
}

/// The figures of a `tcp_info` the kernel filled `length` bytes of.
#[cfg(target_os = "linux")]
fn reading_from(info: &libc::tcp_info, length: usize) -> Option<TcpReading> {
    let needed =
        std::mem::offset_of!(libc::tcp_info, tcpi_data_segs_out) + std::mem::size_of::<u32>();
    (length >= needed).then(|| TcpReading {
        rtt_us: measured_rtt(info.tcpi_min_rtt),
        data_segments_sent: u64::from(info.tcpi_data_segs_out),
        retransmitted_segments: u64::from(info.tcpi_total_retrans),
    })
}

#[cfg(not(target_os = "linux"))]
pub fn read(_fd: RawFd) -> Option<TcpReading> {
    None
}

/// Every established TCP socket on this host, by its remote address (an IPv4-mapped
/// address as plain IPv4, which is how a proxy reports it). A proxy on this host keeps
/// one socket per player, so the player's address and source port find the path to them.
#[cfg(target_os = "linux")]
pub fn read_by_peer() -> io::Result<HashMap<SocketAddr, TcpReading>> {
    let mut readings = HashMap::new();
    for family in [libc::AF_INET, libc::AF_INET6] {
        sock_diag::dump(family as u8, &mut readings)?;
    }
    Ok(readings)
}

#[cfg(not(target_os = "linux"))]
pub fn read_by_peer() -> io::Result<HashMap<SocketAddr, TcpReading>> {
    Ok(HashMap::new())
}

/// A `NETLINK_SOCK_DIAG` dump of TCP sockets with their `tcp_info` (`linux/inet_diag.h`).
#[cfg(target_os = "linux")]
mod sock_diag {
    use std::collections::HashMap;
    use std::io;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    use super::{TcpReading, reading_from};

    const SOCK_DIAG_BY_FAMILY: u16 = 20;
    /// The `inet_diag_msg` attribute holding a `tcp_info`.
    const INET_DIAG_INFO: u16 = 2;
    const TCP_ESTABLISHED: u32 = 1;
    /// `nlmsghdr`, `inet_diag_sockid`, `inet_diag_req_v2` and `inet_diag_msg` sizes.
    const HEADER: usize = 16;
    const SOCKET_ID: usize = 48;
    const REQUEST: usize = 8 + SOCKET_ID;
    const MESSAGE: usize = 4 + SOCKET_ID + 20;
    /// Larger than the biggest datagram the kernel sends a dump in (32 KiB), so none is
    /// cut short.
    const RECEIVE_BYTES: usize = 64 * 1024;

    /// Adds every established TCP socket of `family` to `readings`.
    pub fn dump(family: u8, readings: &mut HashMap<SocketAddr, TcpReading>) -> io::Result<()> {
        // SAFETY: plain syscalls on a descriptor this function owns; the buffers outlive
        // each call and their lengths are passed with them.
        let fd = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
                libc::NETLINK_SOCK_DIAG,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let socket = unsafe { OwnedFd::from_raw_fd(fd) };
        let request = request(family);
        let mut kernel: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        kernel.nl_family = libc::AF_NETLINK as libc::sa_family_t;
        let sent = unsafe {
            libc::sendto(
                socket.as_raw_fd(),
                request.as_ptr().cast(),
                request.len(),
                0,
                (&raw const kernel).cast(),
                std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };
        if sent < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0u8; RECEIVE_BYTES];
        loop {
            let received = unsafe {
                libc::recv(
                    socket.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    0,
                )
            };
            if received < 0 {
                return Err(io::Error::last_os_error());
            }
            if parse(&buffer[..received as usize], readings)? {
                return Ok(());
            }
        }
    }

    /// A dump request for `family`'s established TCP sockets with their `tcp_info`.
    fn request(family: u8) -> [u8; HEADER + REQUEST] {
        let mut request = [0u8; HEADER + REQUEST];
        let flags = (libc::NLM_F_REQUEST | libc::NLM_F_DUMP) as u16;
        request[0..4].copy_from_slice(&((HEADER + REQUEST) as u32).to_ne_bytes());
        request[4..6].copy_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
        request[6..8].copy_from_slice(&flags.to_ne_bytes());
        request[HEADER] = family;
        request[HEADER + 1] = libc::IPPROTO_TCP as u8;
        request[HEADER + 2] = 1 << (INET_DIAG_INFO - 1);
        request[HEADER + 4..HEADER + 8].copy_from_slice(&(1u32 << TCP_ESTABLISHED).to_ne_bytes());
        request
    }

    /// Adds the sockets in one datagram of the dump; `true` once the dump is done.
    pub(super) fn parse(
        datagram: &[u8],
        readings: &mut HashMap<SocketAddr, TcpReading>,
    ) -> io::Result<bool> {
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "malformed sock_diag reply");
        let mut rest = datagram;
        while rest.len() >= HEADER {
            let length = u32::from_ne_bytes(rest[0..4].try_into().unwrap()) as usize;
            let kind = u16::from_ne_bytes(rest[4..6].try_into().unwrap());
            if length < HEADER || length > rest.len() {
                return Err(invalid());
            }
            let body = &rest[HEADER..length];
            match i32::from(kind) {
                libc::NLMSG_DONE => return Ok(true),
                libc::NLMSG_ERROR => {
                    let code = body
                        .get(0..4)
                        .map(|code| i32::from_ne_bytes(code.try_into().unwrap()))
                        .ok_or_else(invalid)?;
                    return Err(io::Error::from_raw_os_error(-code));
                }
                _ if kind == SOCK_DIAG_BY_FAMILY => {
                    if let Some((peer, reading)) = socket(body) {
                        readings.insert(peer, reading);
                    }
                }
                _ => {}
            }
            rest = &rest[aligned(length).min(rest.len())..];
        }
        Ok(false)
    }

    /// One `inet_diag_msg`: the socket's remote address and its `tcp_info` figures.
    fn socket(message: &[u8]) -> Option<(SocketAddr, TcpReading)> {
        let id = message.get(4..4 + SOCKET_ID)?;
        let port = u16::from_be_bytes(id[2..4].try_into().unwrap());
        let destination: [u8; 16] = id[20..36].try_into().unwrap();
        let ip = match i32::from(message[0]) {
            libc::AF_INET => IpAddr::V4(Ipv4Addr::from(
                <[u8; 4]>::try_from(&destination[..4]).unwrap(),
            )),
            libc::AF_INET6 => Ipv6Addr::from(destination).to_canonical(),
            _ => return None,
        };
        let mut attributes = message.get(MESSAGE..)?;
        while attributes.len() >= 4 {
            let length = usize::from(u16::from_ne_bytes(attributes[0..2].try_into().unwrap()));
            let kind = u16::from_ne_bytes(attributes[2..4].try_into().unwrap());
            if length < 4 || length > attributes.len() {
                return None;
            }
            if kind == INET_DIAG_INFO {
                let payload = &attributes[4..length];
                // SAFETY: copies at most the struct's size into a zeroed `tcp_info`, which
                // is plain integers.
                let mut info: libc::tcp_info = unsafe { std::mem::zeroed() };
                let copied = payload.len().min(std::mem::size_of::<libc::tcp_info>());
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        payload.as_ptr(),
                        (&raw mut info).cast::<u8>(),
                        copied,
                    );
                }
                return Some((SocketAddr::new(ip, port), reading_from(&info, copied)?));
            }
            attributes = &attributes[aligned(length).min(attributes.len())..];
        }
        None
    }

    /// Netlink messages and attributes start on 4-byte boundaries.
    fn aligned(length: usize) -> usize {
        length.div_ceil(4) * 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retransmits_are_a_share_of_the_data_segments_sent() {
        let reading = TcpReading {
            rtt_us: Some(85_400),
            data_segments_sent: 800,
            retransmitted_segments: 6,
        };
        assert_eq!(reading.rtt_ms(), Some(85.4));
        assert_eq!(reading.retransmit_percent(), 0.75);
        assert_eq!(TcpReading::default().retransmit_percent(), 0.0);
    }

    #[test]
    fn the_kernels_initial_minimum_is_no_round_trip() {
        assert_eq!(measured_rtt(u32::MAX), None);
        assert_eq!(measured_rtt(0), Some(0), "loopback");
        assert_eq!(measured_rtt(85_400), Some(85_400));
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

    #[cfg(target_os = "linux")]
    #[test]
    fn the_hosts_sockets_are_found_by_their_peer() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server.write_all(b"snapshot").unwrap();
        let mut received = [0; 8];
        client.read_exact(&mut received).unwrap();
        let readings = read_by_peer().expect("Linux lists its TCP sockets");
        // The server's socket is the one whose peer is the client, as a proxy's socket
        // to a player is found by the player's address.
        let reading = readings[&client.local_addr().unwrap()];
        assert!(reading.data_segments_sent >= 1, "{reading:?}");
        assert_eq!(reading.retransmitted_segments, 0);
        assert!(reading.rtt_us.is_some(), "the handshake timed a round trip");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_dump_reply_reports_kernel_errors_and_its_end() {
        let mut readings = HashMap::new();
        let message = |kind: u16, body: &[u8]| {
            let mut message = ((16 + body.len()) as u32).to_ne_bytes().to_vec();
            message.extend(kind.to_ne_bytes());
            message.extend([0; 10]);
            message.extend(body);
            message
        };
        let done = message(libc::NLMSG_DONE as u16, &0i32.to_ne_bytes());
        assert!(sock_diag::parse(&done, &mut readings).unwrap());
        let refused = message(libc::NLMSG_ERROR as u16, &(-libc::EPERM).to_ne_bytes());
        let error = sock_diag::parse(&refused, &mut readings).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EPERM));
        assert!(
            sock_diag::parse(&done[..18], &mut readings).is_err(),
            "truncated"
        );
        assert!(readings.is_empty());
    }
}
