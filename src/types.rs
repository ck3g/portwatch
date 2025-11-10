use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Proto {
    Tcp,
    Udp,
}

impl fmt::Display for Proto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Proto::Tcp => "TCP",
            Proto::Udp => "UDP",
        };
        f.pad(s)
    }
}

#[derive(Debug, Clone)]
pub struct PortProc {
    pub proto: Proto,
    pub local_address: String,
    pub pid: u32,
    pub proc_name: String,
    pub state: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_display() {
        assert_eq!(Proto::Tcp.to_string(), "TCP");
        assert_eq!(Proto::Udp.to_string(), "UDP");
    }

    #[test]
    fn proto_display_with_width() {
        assert_eq!(format!("{:<6}", Proto::Tcp), "TCP   ");
        assert_eq!(format!("{:>6}", Proto::Udp), "   UDP");
    }
}
