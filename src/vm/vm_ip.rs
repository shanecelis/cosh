use std::net::{Ipv4Addr, Ipv6Addr};

use ipnet::{Ipv4Net, Ipv6Net};
use num_bigint::{BigInt, BigUint};
use num_traits::{FromPrimitive, ToPrimitive, Zero};

use chunk::{Ipv4Range, Ipv6Range};
use vm::*;

fn ipv4_addr_to_int(ipv4: Ipv4Addr) -> u32 {
    let octets = ipv4.octets();
    let mut n: u32 = 0;
    for i in 0..4 {
        let next = octets[i].to_u32().unwrap() << (32 - ((i + 1) * 8));
        n = n | next;
    }
    return n;
}

fn ipv6_addr_to_int(ipv6: Ipv6Addr) -> BigUint {
    let octets = ipv6.octets();
    let mut n = BigUint::zero();
    for i in 0..16 {
        let next = BigUint::from(octets[i]) << (128 - ((i + 1) * 8));
        n = n | next;
    }
    return n;
}

fn int_to_ipv4_addr(n: u32) -> Ipv4Addr {
    let o1 = (n >> 24 & 0xFF).to_u8().unwrap();
    let o2 = (n >> 16 & 0xFF).to_u8().unwrap();
    let o3 = (n >> 8  & 0xFF).to_u8().unwrap();
    let o4 = (n       & 0xFF).to_u8().unwrap();
    let ipv4 = Ipv4Addr::new(o1, o2, o3, o4);
    return ipv4;
}

fn int_to_ipv6_addr(n: BigUint) -> Ipv6Addr {
    let mask = BigUint::from_u32(0xFFFF).unwrap();
    let o1 = (n.clone() >> 112u16 & mask.clone()).to_u16().unwrap();
    let o2 = (n.clone() >> 96u16  & mask.clone()).to_u16().unwrap();
    let o3 = (n.clone() >> 80u16  & mask.clone()).to_u16().unwrap();
    let o4 = (n.clone() >> 64u16  & mask.clone()).to_u16().unwrap();
    let o5 = (n.clone() >> 48u16  & mask.clone()).to_u16().unwrap();
    let o6 = (n.clone() >> 32u16  & mask.clone()).to_u16().unwrap();
    let o7 = (n.clone() >> 16u16  & mask.clone()).to_u16().unwrap();
    let o8 = (n.clone()           & mask.clone()).to_u16().unwrap();
    let ipv6 = Ipv6Addr::new(o1, o2, o3, o4, o5, o6, o7, o8);
    return ipv6;
}

impl VM {
    /// Parses an IP address or range and returns an IP object.
    pub fn core_ip(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip requires one argument");
            return 0;
        }

        let value_rr = self.stack.pop().unwrap();
        let value_opt: Option<&str>;
        to_str!(value_rr, value_opt);

        match value_opt {
            Some(s) => {
                if s.contains(".") {
                    if s.contains("-") {
                        let mut iter = s.split("-");
                        let fst = iter.next();
                        if fst.is_none() {
                            self.print_error("unable to parse IP address");
                            return 0;
                        }
                        let snd = iter.next();
                        if snd.is_none() {
                            self.print_error("unable to parse IP address");
                            return 0;
                        }
                        if !iter.next().is_none() {
                            self.print_error("unable to parse IP address");
                            return 0;
                        }

                        let fst_str = fst.unwrap().trim();
                        let snd_str = snd.unwrap().trim();

                        let fst_str_wp = format!("{}/32", fst_str);
                        let snd_str_wp = format!("{}/32", snd_str);

                        let ipv4_fst = Ipv4Net::from_str(&fst_str_wp);
                        let ipv4_snd = Ipv4Net::from_str(&snd_str_wp);

                        match (ipv4_fst, ipv4_snd) {
                            (Ok(ipv4_fst_obj), Ok(ipv4_snd_obj)) => {
                                if !(ipv4_fst_obj < ipv4_snd_obj) {
                                    self.print_error("unable to parse IP address");
                                    return 0;
                                }
                                self.stack.push(
                                    Value::Ipv4Range(
                                        Ipv4Range::new(ipv4_fst_obj.network(),
                                                       ipv4_snd_obj.network())
                                    )
                                );
                                return 1;
                            }
                            (_, _) => {
                                self.print_error("unable to parse IP address");
                                return 0;
                            }
                        }
                    } else {
                        let ipv4_res;
                        if !s.contains("/") {
                            let s2 = format!("{}/32", s);
                            ipv4_res = Ipv4Net::from_str(&s2);
                        } else {
                            ipv4_res = Ipv4Net::from_str(s);
                        }
                        match ipv4_res {
                            Ok(ipv4) => {
                                let addr = ipv4.addr();
                                let addr_int = ipv4_addr_to_int(addr);
                                let prefix_len = ipv4.prefix_len();
                                if prefix_len == 0 && addr_int != 0 {
                                    self.print_error("invalid prefix length");
                                    return 0;
                                }
                                if !(prefix_len == 0 && addr_int == 0) {
                                    let addr_check =
                                        addr_int & (1 << (32 - prefix_len)) - 1;
                                    if addr_check != 0 {
                                        self.print_error("invalid prefix length");
                                        return 0;
                                    }
                                }
                                self.stack.push(Value::Ipv4(ipv4));
                                return 1;
                            }
                            Err(e) => {
                                let err_str =
                                    format!("unable to parse IP address: {}",
                                            e.to_string());
                                self.print_error(&err_str);
                                return 0;
                            }
                        }
                    }
                } else {
                    if s.contains("-") {
                        let mut iter = s.split("-");
                        let fst = iter.next();
                        if fst.is_none() {
                            self.print_error("unable to parse IP address");
                            return 0;
                        }
                        let snd = iter.next();
                        if snd.is_none() {
                            self.print_error("unable to parse IP address");
                            return 0;
                        }
                        if !iter.next().is_none() {
                            self.print_error("unable to parse IP address");
                            return 0;
                        }

                        let fst_str = fst.unwrap().trim();
                        let snd_str = snd.unwrap().trim();

                        let fst_str_wp = format!("{}/128", fst_str);
                        let snd_str_wp = format!("{}/128", snd_str);

                        let ipv6_fst = Ipv6Net::from_str(&fst_str_wp);
                        let ipv6_snd = Ipv6Net::from_str(&snd_str_wp);

                        match (ipv6_fst, ipv6_snd) {
                            (Ok(ipv6_fst_obj), Ok(ipv6_snd_obj)) => {
                                if !(ipv6_fst_obj < ipv6_snd_obj) {
                                    self.print_error("unable to parse IP address");
                                    return 0;
                                }
                                self.stack.push(
                                    Value::Ipv6Range(
                                        Ipv6Range::new(ipv6_fst_obj.network(),
                                                       ipv6_snd_obj.network())
                                    )
                                );
                                return 1;
                            }
                            (_, _) => {
                                self.print_error("unable to parse IP address");
                                return 0;
                            }
                        }
                    } else {
                        let ipv6_res;
                        if !s.contains("/") {
                            let s2 = format!("{}/128", s);
                            ipv6_res = Ipv6Net::from_str(&s2);
                        } else {
                            ipv6_res = Ipv6Net::from_str(s);
                        }
                        match ipv6_res {
                            Ok(ipv6) => {
                                let addr = ipv6.addr();
                                let addr_int = ipv6_addr_to_int(addr);
                                let prefix_len = ipv6.prefix_len();
                                if prefix_len == 0 && !addr_int.is_zero() {
                                    self.print_error("invalid prefix length");
                                    return 0;
                                }
                                if !(prefix_len == 0
                                        && addr_int == BigUint::from(0u8)) {
                                    let prefix_mask =
                                        (BigUint::from(1u8)
                                            << (128 - prefix_len))
                                            - BigUint::from(1u8);
                                    let addr_check: BigUint =
                                        addr_int & prefix_mask;
                                    if !addr_check.is_zero() {
                                        self.print_error("invalid prefix length");
                                        return 0;
                                    }
                                }
                                self.stack.push(Value::Ipv6(ipv6));
                                return 1;
                            }
                            Err(e) => {
                                let err_str =
                                    format!("unable to parse IP address: {}",
                                            e.to_string());
                                self.print_error(&err_str);
                                return 0;
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        return 1;
    }

    /// Converts an integer into an IP object.
    pub fn core_ip_from_int(&mut self) -> i32 {
        if self.stack.len() < 2 {
            self.print_error("ip.from-int requires two arguments");
            return 0;
        }

        let value_rr = self.stack.pop().unwrap();
        let value_opt = value_rr.to_bigint();

        let version_rr = self.stack.pop().unwrap();
        let version_opt = version_rr.to_int();

        match (version_opt, value_opt) {
            (Some(4), Some(value)) => {
                if value > BigInt::from_u32(0xFFFFFFFF).unwrap() {
                    self.print_error("IPv4 address is outside 32-bit bound");
                    return 0;
                }
                let uvalue =
                    value.to_biguint().unwrap().to_u32().unwrap();
                let ipv4 = int_to_ipv4_addr(uvalue);
                self.stack.push(Value::Ipv4(Ipv4Net::new(ipv4, 32).unwrap()));
            }
            (Some(6), Some(value)) => {
                let uvalue = value.to_biguint().unwrap();
                let ipv6 = int_to_ipv6_addr(uvalue);
                self.stack.push(Value::Ipv6(Ipv6Net::new(ipv6, 128).unwrap()));
            }
            (Some(_), _) => {
                self.print_error("invalid IP address version");
                return 0;
            }
            _ => {
                self.print_error("invalid IP integer");
                return 0;
            }
        }

        return 1;
    }

    /// Returns the first address of an IP object.
    pub fn core_ip_addr(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.addr requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        let ip_str;
        match ip_rr {
            Value::Ipv4(ipv4net) => {
                ip_str = format!("{}", ipv4net);
            }
            Value::Ipv4Range(ipv4range) => {
                ip_str = format!("{}", ipv4range.s);
            }
            Value::Ipv6(ipv6net) => {
                ip_str = format!("{}", ipv6net);
            }
            Value::Ipv6Range(ipv6range) => {
                ip_str = format!("{}", ipv6range.s);
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }

        let ip_str_no_len =
            ip_str.chars().take_while(|&c| c != '/').collect::<String>();
        let sp = StringPair::new(ip_str_no_len.to_string(), None);
        let st = Value::String(Rc::new(RefCell::new(sp)));
        self.stack.push(st);
        return 1;
    }

    /// Returns the prefix length of an IP object.
    pub fn core_ip_len(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.len requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        match ip_rr {
            Value::Ipv4(ipv4net) => {
                let len = Value::Int(ipv4net.prefix_len().into());
                self.stack.push(len);
                return 1;
            }
            Value::Ipv4Range(ipv4range) => {
                let s = ipv4range.s;
                let e = ipv4range.e;
                let s_num = ipv4_addr_to_int(s);
                let e_num = ipv4_addr_to_int(e);
                if s_num == 0 && e_num == 0xFFFFFFFF {
                    self.stack.push(Value::Int(0));
                    return 1;
                }
                let mut host_count = e_num - s_num + 1;
                let mut len = 32;
                if host_count & (host_count - 1) == 0 {
                    loop {
                        if host_count == 1 {
                            break;
                        } else {
                            host_count = host_count >> 1;
                            len = len - 1;
                        }
                    }
                    self.stack.push(Value::Int(len));
                    return 1;
                } else {
                    self.print_error("IP range has no length");
                    return 0;
                }
            }
            Value::Ipv6(ipv6net) => {
                let len = Value::Int(ipv6net.prefix_len().into());
                self.stack.push(len);
                return 1;
            }
            Value::Ipv6Range(ipv6range) => {
                let s = ipv6range.s;
                let e = ipv6range.e;
                let s_num = ipv6_addr_to_int(s);
                let e_num = ipv6_addr_to_int(e);
                let zero = BigUint::zero();
                let one = BigUint::from(1u8);
                let mut host_count = e_num - s_num + one.clone();
                let mut len = 128;
                if host_count.clone() & (host_count.clone() - one.clone()) == zero {
                    loop {
                        if host_count == one {
                            break;
                        } else {
                            host_count = host_count.clone() >> 1;
                            len = len - 1;
                        }
                    }
                    self.stack.push(Value::Int(len));
                    return 1;
                } else {
                    self.print_error("IP range has no length");
                    return 0;
                }
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }
    }

    /// Returns the first address of the IP object as an integer.
    pub fn core_ip_addr_int(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.addr-int requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        match ip_rr {
            Value::Ipv4(ipv4net) => {
                let ipv4addr_int =
                    ipv4_addr_to_int(ipv4net.network());
                let ipv4addr_val = Value::BigInt(BigInt::from(ipv4addr_int));
                self.stack.push(ipv4addr_val);
                return 1;
            }
            Value::Ipv4Range(ipv4range) => {
                let ipv4addr_int =
                    ipv4_addr_to_int(ipv4range.s);
                let ipv4addr_val = Value::BigInt(BigInt::from(ipv4addr_int));
                self.stack.push(ipv4addr_val);
                return 1;
            }
            Value::Ipv6(ipv6net) => {
                let ipv6addr_int =
                    ipv6_addr_to_int(ipv6net.network());
                let ipv6addr_val = Value::BigInt(BigInt::from(ipv6addr_int));
                self.stack.push(ipv6addr_val);
                return 1;
            }
            Value::Ipv6Range(ipv6range) => {
                let ipv6addr_int =
                    ipv6_addr_to_int(ipv6range.s);
                let ipv6addr_val = Value::BigInt(BigInt::from(ipv6addr_int));
                self.stack.push(ipv6addr_val);
                return 1;
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }
    }

    /// Returns the last address of the IP object.
    pub fn core_ip_last_addr(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.last-addr requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        match ip_rr {
            Value::Ipv4(ipv4net) => {
                if ipv4_addr_to_int(ipv4net.network()) == 0
                        && ipv4net.prefix_len() == 0 {
                    let lastaddr = format!("{}", "255.255.255.255");
                    let sp = StringPair::new(lastaddr, None);
                    let st = Value::String(Rc::new(RefCell::new(sp)));
                    self.stack.push(st);
                    return 1;
                }
                let ipv4addr_int =
                    ipv4_addr_to_int(ipv4net.network()) |
                        ((1 << (32 - ipv4net.prefix_len())) - 1);
                let lastaddr_int = int_to_ipv4_addr(ipv4addr_int);
                let lastaddr = format!("{}", lastaddr_int);
                let sp = StringPair::new(lastaddr, None);
                let st = Value::String(Rc::new(RefCell::new(sp)));
                self.stack.push(st);
                return 1;
            }
            Value::Ipv4Range(ipv4range) => {
                let ipv4addr_int = ipv4_addr_to_int(ipv4range.e);
                let lastaddr_int = int_to_ipv4_addr(ipv4addr_int);
                let lastaddr = format!("{}", lastaddr_int);
                let sp = StringPair::new(lastaddr, None);
                let st = Value::String(Rc::new(RefCell::new(sp)));
                self.stack.push(st);
            }
            Value::Ipv6(ipv6net) => {
                let prefix_mask =
                    (BigUint::from(1u8) << (128 - ipv6net.prefix_len()))
                        - BigUint::from(1u8);
                let ipv6addr_int =
                    ipv6_addr_to_int(ipv6net.network()) | prefix_mask;
                let lastaddr_int = int_to_ipv6_addr(ipv6addr_int);
                let lastaddr = format!("{}", lastaddr_int);
                let sp = StringPair::new(lastaddr, None);
                let st = Value::String(Rc::new(RefCell::new(sp)));
                self.stack.push(st);
            }
            Value::Ipv6Range(ipv6range) => {
                let ipv6addr_int = ipv6_addr_to_int(ipv6range.e);
                let lastaddr_int = int_to_ipv6_addr(ipv6addr_int);
                let lastaddr = format!("{}", lastaddr_int);
                let sp = StringPair::new(lastaddr, None);
                let st = Value::String(Rc::new(RefCell::new(sp)));
                self.stack.push(st);
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }

        return 1;
    }

    /// Returns the last address of the IP object as an integer.
    pub fn core_ip_last_addr_int(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.last-addr-int requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        match ip_rr {
            Value::Ipv4(ipv4net) => {
                if ipv4_addr_to_int(ipv4net.network()) == 0
                        && ipv4net.prefix_len() == 0 {
                    let lastaddr_val =
                        Value::BigInt(BigInt::from_u32(0xFFFFFFFF).unwrap());
                    self.stack.push(lastaddr_val);
                    return 1;
                }
                let ipv4addr_int =
                    ipv4_addr_to_int(ipv4net.network()) |
                        ((1 << (32 - ipv4net.prefix_len())) - 1);
                let lastaddr_val =
                    Value::BigInt(BigInt::from_u32(ipv4addr_int).unwrap());
                self.stack.push(lastaddr_val);
                return 1;
            }
            Value::Ipv4Range(ipv4range) => {
                let ipv4addr_int = ipv4_addr_to_int(ipv4range.e);
                let lastaddr_val =
                    Value::BigInt(BigInt::from(ipv4addr_int));
                self.stack.push(lastaddr_val);
            }
            Value::Ipv6(ipv6net) => {
                let prefix_mask =
                    (BigUint::from(1u8) << (128 - ipv6net.prefix_len()))
                        - BigUint::from(1u8);
                let ipv6addr_int =
                    ipv6_addr_to_int(ipv6net.network()) | prefix_mask;
                let lastaddr_val =
                    Value::BigInt(BigInt::from(ipv6addr_int));
                self.stack.push(lastaddr_val);
            }
            Value::Ipv6Range(ipv6range) => {
                let ipv6addr_int = ipv6_addr_to_int(ipv6range.e);
                let lastaddr_val =
                    Value::BigInt(BigInt::from(ipv6addr_int));
                self.stack.push(lastaddr_val);
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }

        return 1;
    }

    /// Returns the number of hosts covered by this IP object.
    pub fn core_ip_size(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.size requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        match ip_rr {
            Value::Ipv4(ipv4net) => {
                if ipv4_addr_to_int(ipv4net.network()) == 0
                        && ipv4net.prefix_len() == 0 {
                    let size_val =
                        Value::BigInt(BigInt::from_u32(0xFFFFFFFF).unwrap());
                    self.stack.push(size_val);
                    return 1;
                }
                let ipv4addr_int =
                    ipv4_addr_to_int(ipv4net.network());
                let lastaddr_int =
                    ipv4addr_int | ((1 << (32 - ipv4net.prefix_len())) - 1);
                let size = lastaddr_int - ipv4addr_int + 1;
                let size_val =
                    Value::BigInt(BigInt::from_u32(size).unwrap());
                self.stack.push(size_val);
                return 1;
            }
            Value::Ipv4Range(ipv4range) => {
                let s = ipv4range.s;
                let e = ipv4range.e;
                let s_num = ipv4_addr_to_int(s);
                let e_num = ipv4_addr_to_int(e);
                if s_num == 0 && e_num == 0xFFFFFFFF {
                    self.stack.push(Value::BigInt(BigInt::from_u32(0xFFFFFFFF).unwrap() + BigInt::from(1u8)));
                    return 1;
                }
                let host_count = e_num - s_num + 1;
                self.stack.push(Value::BigInt(BigInt::from_u32(host_count).unwrap()));
                return 1;
            }
            Value::Ipv6(ipv6net) => {
                let prefix_mask =
                    (BigUint::from(1u8) << (128 - ipv6net.prefix_len()))
                        - BigUint::from(1u8);
                let ipv6addr_int =
                    ipv6_addr_to_int(ipv6net.network());
                let lastaddr_int =
                    ipv6addr_int.clone() | prefix_mask;
                let size = lastaddr_int - ipv6addr_int + BigUint::from(1u8);
                let size_val =
                    Value::BigInt(BigInt::from(size));
                self.stack.push(size_val);
            }
            Value::Ipv6Range(ipv6range) => {
                let s = ipv6range.s;
                let e = ipv6range.e;
                let s_num = ipv6_addr_to_int(s);
                let e_num = ipv6_addr_to_int(e);
                let host_count = e_num - s_num + BigUint::from(1u8);
                self.stack.push(Value::BigInt(BigInt::from(host_count)));
                return 1;
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }

        return 1;
    }

    /// Returns the IP object version.
    pub fn core_ip_version(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.version requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        match ip_rr {
            Value::Ipv4(_) => {
                self.stack.push(Value::Int(4));
                return 1;
            }
            Value::Ipv4Range(_) => {
                self.stack.push(Value::Int(4));
                return 1;
            }
            Value::Ipv6(_) => {
                self.stack.push(Value::Int(6));
                return 1;
            }
            Value::Ipv6Range(_) => {
                self.stack.push(Value::Int(6));
                return 1;
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }
    }

    /// Returns the IP object as a string.
    pub fn core_ip_to_string(&mut self) -> i32 {
        if self.stack.len() < 1 {
            self.print_error("ip.version requires one argument");
            return 0;
        }

        let ip_rr = self.stack.pop().unwrap();
        match ip_rr {
            Value::Ipv4(ipv4net) => {
                let prefix_len = ipv4net.prefix_len();
                if prefix_len == 32 {
                    let ip_str = format!("{}", ipv4net);
                    let ip_str_no_len =
                        ip_str.chars().take_while(|&c| c != '/')
                                      .collect::<String>();
                    let sp = StringPair::new(ip_str_no_len.to_string(), None);
                    let st = Value::String(Rc::new(RefCell::new(sp)));
                    self.stack.push(st);
                } else {
                    let ip_str = format!("{}", ipv4net);
                    let sp = StringPair::new(ip_str, None);
                    let st = Value::String(Rc::new(RefCell::new(sp)));
                    self.stack.push(st);
                }
                return 1;
            }
            Value::Ipv4Range(ipv4range) => {
                let ip_str = format!("{}-{}", ipv4range.s,
                                              ipv4range.e);
                let sp = StringPair::new(ip_str.to_string(), None);
                let st = Value::String(Rc::new(RefCell::new(sp)));
                self.stack.push(st);
                return 1;
            }
            Value::Ipv6(ipv6net) => {
                let prefix_len = ipv6net.prefix_len();
                if prefix_len == 128 {
                    let ip_str = format!("{}", ipv6net.network());
                    let sp = StringPair::new(ip_str, None);
                    let st = Value::String(Rc::new(RefCell::new(sp)));
                    self.stack.push(st);
                } else {
                    let ip_str =
                        format!("{}/{}",
                                ipv6net.network(), ipv6net.prefix_len());
                    let sp = StringPair::new(ip_str, None);
                    let st = Value::String(Rc::new(RefCell::new(sp)));
                    self.stack.push(st);
                }
                return 1;
            }
            Value::Ipv6Range(ipv6range) => {
                let ip_str = format!("{}-{}", ipv6range.s,
                                              ipv6range.e);
                let sp = StringPair::new(ip_str.to_string(), None);
                let st = Value::String(Rc::new(RefCell::new(sp)));
                self.stack.push(st);
                return 1;
            }
            _ => {
                self.print_error("expected IP object argument");
                return 0;
            }
        }
    }
}
