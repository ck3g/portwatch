use crate::types::{PortProc, Proto};
use anyhow::{Context, Result};
use std::process::Command;
use which::which;

pub trait PortScanner {
    fn scan(&mut self) -> Result<Vec<PortProc>>;
}

pub struct SsScanner;

impl SsScanner {
    fn fetch_output(&self) -> Result<String> {
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

    fn scan_ports(&self, output: String) -> Vec<PortProc> {
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

    fn parse_process_info(&self, proc_str: &str) -> Option<(String, u32)> {
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

pub struct LsofScanner;

impl LsofScanner {
    fn fetch_output(&self) -> Result<String> {
        let output = Command::new("lsof")
            .args(["-nP", "-i"])
            .output()
            .context("failed to spawn lsof -nP -i")?;

        anyhow::ensure!(
            output.status.success(),
            "lsof exited with status {}\nStderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn scan_ports(&self, output: String) -> Vec<PortProc> {
        let mut results: Vec<PortProc> = Vec::new();

        for line in output.lines() {
            // Skip header
            if line.starts_with("COMMAND") {
                continue;
            }

            if line.contains("->") {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 9 {
                continue;
            }

            let pid = match parts[1].parse::<u32>() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let p = PortProc {
                proto: if parts[7] == "TCP" {
                    Proto::Tcp
                } else {
                    Proto::Udp
                },
                local_address: parts[8].to_string(),
                pid,
                proc_name: parts[0].to_string(),
                state: if parts[7] == "TCP" {
                    Some("LISTEN".to_string())
                } else {
                    Some("UNCONN".to_string())
                },
            };

            results.push(p);
        }

        results
    }
}

pub fn create_scanner() -> Result<Box<dyn PortScanner>> {
    // Fallback to lsof (works on Linux and macOS)
    if which("lsof").is_ok() {
        return Ok(Box::new(LsofScanner));
    }
    // Try ss first (faster on Linux)
    if which("ss").is_ok() {
        return Ok(Box::new(SsScanner));
    }

    // No supported tool found
    anyhow::bail!(
        "Error: No supported port scanning tool found\n\
        \n\
        portwatch requires either 'ss' or 'lsof' to scan network ports.\n\
        \n\
        Install one of the following:\n\
          - Linux (ss):    sudo apt install iproute2      (Debian/Ubuntu)\n\
                           sudo dnf install iproute2      (Fedora/RHEL)\n\
                           sudo pacman -S iproute2        (Arch Linux)\n\
          - Linux/macOS:   sudo apt install lsof          (Debian/Ubuntu)\n\
                           sudo pacman -S lsof            (Arch Linux)\n\
                           (lsof is pre-installed on macOS)"
    )
}

pub fn extract_port(local_address: &str) -> Option<u16> {
    local_address
        .rsplit_once(":")
        .and_then(|(_, port_str)| port_str.parse::<u16>().ok())
}

impl PortScanner for SsScanner {
    fn scan(&mut self) -> Result<Vec<PortProc>> {
        let output = self.fetch_output()?;
        let mut items = self.scan_ports(output);
        items.sort_by_key(|p| extract_port(&p.local_address));
        Ok(items)
    }
}

impl PortScanner for LsofScanner {
    fn scan(&mut self) -> Result<Vec<PortProc>> {
        let output = self.fetch_output()?;
        let mut items = self.scan_ports(output);
        items.sort_by_key(|p| extract_port(&p.local_address));
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod ss_scanner {
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
    }

    mod lsof_scanner {
        use super::*;

        const SAMPLE_LSOF_OUTPUT: &str = r#"COMMAND     PID USER  FD   TYPE DEVICE SIZE/OFF NODE NAME
chromium  40269 ck3g 255u  IPv4 183157      0t0  UDP 224.0.0.251:5353
chromium  40316 ck3g  18u  IPv4 184101      0t0  TCP 192.168.1.148:37114->172.217.218.188:5228 (ESTABLISHED)
chromium  40316 ck3g  24u  IPv4 186798      0t0  TCP 192.168.1.148:34838->76.76.21.21:443 (ESTABLISHED)
chromium  40316 ck3g  31u  IPv4 198955      0t0  TCP 192.168.1.148:38592->140.82.114.26:443 (ESTABLISHED)
chromium  40316 ck3g  38u  IPv4 202589      0t0  TCP 192.168.1.148:36062->172.217.16.74:443 (ESTABLISHED)
chromium  40316 ck3g  44u  IPv4 190513      0t0  UDP 192.168.1.148:33558->172.217.16.74:443
chromium  40316 ck3g  49u  IPv4 197361      0t0  TCP 192.168.1.148:40822->142.251.142.37:443 (ESTABLISHED)
chromium  40316 ck3g  79u  IPv4 114308      0t0  UDP 224.0.0.251:5353
MainThrea 66212 ck3g  34u  IPv4 192336      0t0  TCP 192.168.1.148:34128->100.30.82.226:443 (ESTABLISHED)
bundle    75423 ck3g   6u  IPv4 194742      0t0  TCP 127.0.0.1:3000 (LISTEN)
bundle    75423 ck3g   7u  IPv6 194743      0t0  TCP [::1]:3000 (LISTEN)
"#;

        #[test]
        fn scan_ports_with_multiple_entries() {
            let scanner = LsofScanner;
            let results = scanner.scan_ports(SAMPLE_LSOF_OUTPUT.to_string());
            assert_eq!(results.len(), 4);

            assert_eq!(results[0].proto, Proto::Udp);
            assert_eq!(results[0].local_address, "224.0.0.251:5353");
            assert_eq!(results[0].pid, 40269);
            assert_eq!(results[0].proc_name, "chromium");
            assert_eq!(results[0].state, Some("UNCONN".to_string()));

            assert_eq!(results[1].proto, Proto::Udp);
            assert_eq!(results[1].local_address, "224.0.0.251:5353");
            assert_eq!(results[1].pid, 40316);
            assert_eq!(results[1].proc_name, "chromium");
            assert_eq!(results[1].state, Some("UNCONN".to_string()));

            assert_eq!(results[2].proto, Proto::Tcp);
            assert_eq!(results[2].local_address, "127.0.0.1:3000");
            assert_eq!(results[2].pid, 75423);
            assert_eq!(results[2].proc_name, "bundle");
            assert_eq!(results[2].state, Some("LISTEN".to_string()));

            assert_eq!(results[3].proto, Proto::Tcp);
            assert_eq!(results[3].local_address, "[::1]:3000");
            assert_eq!(results[3].pid, 75423);
            assert_eq!(results[3].proc_name, "bundle");
            assert_eq!(results[3].state, Some("LISTEN".to_string()));
        }
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
