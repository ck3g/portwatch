use crate::types::{PortProc, Proto};
use anyhow::{Context, Result};
use std::process::Command;
use which::which;

pub trait PortScanner {
    fn scan(&mut self) -> Result<Vec<PortProc>>;
}

pub struct SsScanner;

impl SsScanner {
    pub fn check_ss_available(&self) -> Result<()> {
        which("ss").context("ss command not found")?;
        Ok(())
    }

    pub fn fetch_output(&self) -> Result<String> {
        let output = Command::new("ss")
            .args(["-tulpn"])
            .output()
            .context("failed to spawn ss -tulpn")?;

        anyhow::ensure!(
            output.status.success(),
            "ss exited with status {}\nStderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn scan_ports(&self, output: String) -> Vec<PortProc> {
        let mut results: Vec<PortProc> = Vec::new();

        for line in output.lines() {
            // Skip header
            if line.starts_with("Netid") {
                continue;
            }

            // Skip lines without process info
            if !line.contains("users:((") {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();

            let (proc_name, pid) = if let Some(info) = self.parse_process_info(parts[6]) {
                info
            } else {
                // Skip line if parse failed
                continue;
            };

            let p = PortProc {
                proto: if parts[0] == "tcp" {
                    Proto::Tcp
                } else {
                    Proto::Udp
                },
                local_address: parts[4].to_string(),
                pid,
                proc_name,
                state: Some(parts[1].to_string()),
            };

            results.push(p);
        }

        results
    }

    pub fn parse_process_info(&self, proc_str: &str) -> Option<(String, u32)> {
        let name_start = proc_str.find("((\"")? + 3;
        let name_end = proc_str[name_start..].find("\",")?;
        let proc_name = &proc_str[name_start..name_start + name_end];

        let pid_start = proc_str.find("pid=")? + 4;
        let pid_end = proc_str[pid_start..].find(",")?;
        let pid_str = &proc_str[pid_start..pid_start + pid_end];
        let pid = pid_str.parse::<u32>().ok()?;

        Some((proc_name.to_string(), pid))
    }
}

pub fn extract_port(local_address: &str) -> Option<u16> {
    local_address
        .rsplit_once(":")
        .and_then(|(_, port_str)| port_str.parse::<u16>().ok())
}

// impl PortScanner for SsScanner {
//     fn scan(&mut self) -> Result<Vec<PortProc>> {
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SS_OUTPUT: &str = r#"Netid       State        Recv-Q       Send-Q                     Local Address:Port                Peer Address:Port       Process
udp         UNCONN       0            0                            224.0.0.251:5353                     0.0.0.0:*           users:(("chromium",pid=2440,fd=208))
udp         UNCONN       0            0                            224.0.0.251:5353                     0.0.0.0:*           users:(("chromium",pid=2523,fd=57))
udp         UNCONN       0            0                          127.0.0.53%lo:53                       0.0.0.0:*
udp         UNCONN       0            0                    192.168.1.148%wlan0:68                       0.0.0.0:*
tcp         LISTEN       0            4096                           127.0.0.1:45549                    0.0.0.0:*
tcp         LISTEN       0            1024                           127.0.0.1:3000                     0.0.0.0:*           users:(("ruby",pid=27264,fd=6))
tcp         LISTEN       0            1024                               [::1]:3000                        [::]:*           users:(("ruby",pid=27264,fd=7))
"#;

    #[test]
    fn parse_process_info_success() {
        let proc_str = "users:((\"chromium\",pid=2440,fd=237))";
        let proc_name = String::from("chromium");
        let pid: u32 = 2440;
        let scanner = SsScanner;

        assert_eq!(scanner.parse_process_info(proc_str), Some((proc_name, pid)));
    }

    #[test]
    fn parse_process_info_from_empty_string() {
        let proc_str = "";
        let scanner = SsScanner;
        assert_eq!(scanner.parse_process_info(proc_str), None);
    }

    #[test]
    fn parse_process_info_invalid_structure() {
        let proc_str = "users:\"chromium\",pid=2440,fd=237";
        let scanner = SsScanner;
        assert_eq!(scanner.parse_process_info(proc_str), None);
    }

    #[test]
    fn parse_process_info_invalid_pid() {
        let proc_str = "users:((\"chromium\",pid=abc,fd=237))";
        let scanner = SsScanner;
        assert_eq!(scanner.parse_process_info(proc_str), None);
    }

    #[test]
    fn scan_ports_with_multiple_entries() {
        let scanner = SsScanner;
        let results = scanner.scan_ports(SAMPLE_SS_OUTPUT.to_string());
        assert_eq!(results.len(), 4);

        assert_eq!(results[0].proto, Proto::Udp);
        assert_eq!(results[0].local_address, "224.0.0.251:5353");
        assert_eq!(results[0].pid, 2440);
        assert_eq!(results[0].proc_name, "chromium");
        assert_eq!(results[0].state, Some("UNCONN".to_string()));

        assert_eq!(results[1].proto, Proto::Udp);
        assert_eq!(results[1].local_address, "224.0.0.251:5353");
        assert_eq!(results[1].pid, 2523);
        assert_eq!(results[1].proc_name, "chromium");
        assert_eq!(results[1].state, Some("UNCONN".to_string()));

        assert_eq!(results[2].proto, Proto::Tcp);
        assert_eq!(results[2].local_address, "127.0.0.1:3000");
        assert_eq!(results[2].pid, 27264);
        assert_eq!(results[2].proc_name, "ruby");
        assert_eq!(results[2].state, Some("LISTEN".to_string()));

        assert_eq!(results[3].proto, Proto::Tcp);
        assert_eq!(results[3].local_address, "[::1]:3000");
        assert_eq!(results[3].pid, 27264);
        assert_eq!(results[3].proc_name, "ruby");
        assert_eq!(results[3].state, Some("LISTEN".to_string()));
    }

    #[test]
    fn extract_port_from_local_address() {
        assert_eq!(extract_port(""), None);
        assert_eq!(extract_port("224.0.0.251:5353"), Some(5353));
        assert_eq!(extract_port("[::1]:3000"), Some(3000));
        assert_eq!(extract_port("224.0.0.251:abcd"), None);
        assert_eq!(extract_port("invalid-address"), None);
    }
}
