use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::fmt;
use std::fs::File;
use std::fs::ReadDir;
use std::io::BufReader;
use std::io::BufWriter;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::rc::Rc;
use std::str;

use chrono::prelude::*;
use indexmap::IndexMap;
use ipnet::{Ipv4Net, Ipv6Net};
use iprange::IpRange;
use nonblock::NonBlockingReader;
use num::FromPrimitive;
use num::ToPrimitive;
use num_bigint::BigInt;
use num_traits::Zero;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::process::{ChildStderr, ChildStdout};

use opcode::{to_opcode, OpCode};
use vm::VM;

/// A chunk is a parsed/processed piece of code.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chunk {
    /// The name of the chunk.  Either the name of the file that
    /// contains the associated code, or "(main)", for code entered at
    /// the REPL.
    pub name: String,
    /// The bytecode for the chunk.
    pub data: Vec<u8>,
    /// The line and column number information for the chunk.  The
    /// entries in this vector correspond to the entries in the
    /// bytecode vector.
    pub points: Vec<(u32, u32)>,
    /// The set of constant values for the chunk.
    pub constants: Vec<ValueSD>,
    /// The functions defined within the chunk.
    pub functions: HashMap<String, Rc<RefCell<Chunk>>>,
    #[serde(skip)]
    /// Initialised values for the constants of the chunk.
    pub constant_values: Vec<Value>,
    /// Whether the chunk is for a generator function.
    pub is_generator: bool,
    /// Whether the chunk deals with global variables.
    pub has_vars: bool,
    /// The maximum argument count for a generator function
    /// (only set if is_generator is true).
    pub arg_count: i32,
    /// The required argument count for a generator function
    /// (only set if is_generator is true).
    pub req_arg_count: i32,
    /// Whether the chunk represents a nested function.
    pub nested: bool,
    /// The scope depth for the chunk.
    pub scope_depth: u32,
}

/// StringTriple is used for the core string type.  It binds together
/// a display string (i.e. a raw string), an escaped string (to save
/// repeating that operation), and the corresponding regex (to save
/// regenerating that regex).  The bool flag indicates whether global
/// matching should be used for the regex.  The display string is the
/// 'real' string, and includes e.g. literal newline characters,
/// whereas the escaped string includes escapes for those characters.
#[derive(Debug, Clone)]
pub struct StringTriple {
    pub string: String,
    pub escaped_string: String,
    pub regex: Option<(Rc<Regex>, bool)>,
}

/// Takes a display string and returns an escaped string.
fn escape_string(s: &str) -> String {
    let mut s2 = String::from("");
    let mut next_escaped = false;
    for c in s.chars() {
        if next_escaped {
            match c {
                '\\' => {
                    s2.push('\\');
                }
                '"' => {
                    s2.push('\\');
                    s2.push('\\');
                    s2.push(c);
                }
                _ => {
                    s2.push('\\');
                    s2.push(c);
                }
            }
            next_escaped = false;
        } else {
            match c {
                '\\' => {
                    next_escaped = true;
                }
                '\n' => {
                    s2.push('\\');
                    s2.push('n');
                }
                '\r' => {
                    s2.push('\\');
                    s2.push('r');
                }
                '\t' => {
                    s2.push('\\');
                    s2.push('t');
                }
                '"' => {
                    s2.push('\\');
                    s2.push('"');
                }
                _ => {
                    s2.push(c);
                }
            }
        }
    }
    s2
}

impl StringTriple {
    pub fn new(s: String, r: Option<(Rc<Regex>, bool)>) -> StringTriple {
        let e = escape_string(&s);
        StringTriple {
            string: s,
            escaped_string: e,
            regex: r,
        }
    }
    pub fn new_with_escaped(s: String, e: String, r: Option<(Rc<Regex>, bool)>) -> StringTriple {
        StringTriple {
            string: s,
            escaped_string: e,
            regex: r,
        }
    }
}

/// A generator object, containing a generator chunk along with all of
/// its associated state.
#[derive(Debug, Clone)]
pub struct GeneratorObject {
    /// The local variable stack.
    pub local_vars_stack: Rc<RefCell<Vec<Value>>>,
    /// The current instruction index.
    pub index: usize,
    /// The chunk of the associated generator function.
    pub chunk: Rc<RefCell<Chunk>>,
    /// The chunks of the other functions in the call stack.
    pub call_stack_chunks: Vec<(Rc<RefCell<Chunk>>, usize)>,
    /// The values that need to be passed into the generator when
    /// it is first called.
    pub gen_args: Vec<Value>,
}

impl GeneratorObject {
    /// Construct a generator object.
    pub fn new(
        local_vars_stack: Rc<RefCell<Vec<Value>>>,
        index: usize,
        chunk: Rc<RefCell<Chunk>>,
        call_stack_chunks: Vec<(Rc<RefCell<Chunk>>, usize)>,
        gen_args: Vec<Value>,
    ) -> GeneratorObject {
        GeneratorObject {
            local_vars_stack,
            index,
            chunk,
            call_stack_chunks,
            gen_args,
        }
    }
}

/// A hash object paired with its current index, for use within
/// the various hash generators.
#[derive(Debug, Clone)]
pub struct HashWithIndex {
    pub i: usize,
    pub h: Value,
}

impl HashWithIndex {
    pub fn new(i: usize, h: Value) -> HashWithIndex {
        HashWithIndex { i, h }
    }
}

/// An IPv4 range object.
#[derive(Debug, Clone)]
pub struct Ipv4Range {
    pub s: Ipv4Addr,
    pub e: Ipv4Addr,
}

impl Ipv4Range {
    pub fn new(s: Ipv4Addr, e: Ipv4Addr) -> Ipv4Range {
        Ipv4Range { s, e }
    }
}

/// An IPv6 range object.
#[derive(Debug, Clone)]
pub struct Ipv6Range {
    pub s: Ipv6Addr,
    pub e: Ipv6Addr,
}

impl Ipv6Range {
    pub fn new(s: Ipv6Addr, e: Ipv6Addr) -> Ipv6Range {
        Ipv6Range { s, e }
    }
}

/// An IP set object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpSet {
    pub ipv4: IpRange<Ipv4Net>,
    pub ipv6: IpRange<Ipv6Net>,
}

impl IpSet {
    pub fn new(ipv4: IpRange<Ipv4Net>, ipv6: IpRange<Ipv6Net>) -> IpSet {
        IpSet { ipv4, ipv6 }
    }
}

/// A command generator object.
pub struct CommandGenerator {
    pub stdout: NonBlockingReader<ChildStdout>,
    pub stderr: NonBlockingReader<ChildStderr>,
    pub stdout_buffer: Vec<u8>,
    pub stderr_buffer: Vec<u8>,
    get_stdout: bool,
    get_stderr: bool,
    pub get_combined: bool,
}

impl CommandGenerator {
    pub fn new(
        stdout: NonBlockingReader<ChildStdout>,
        stderr: NonBlockingReader<ChildStderr>,
        get_stdout: bool,
        get_stderr: bool,
        get_combined: bool,
    ) -> CommandGenerator {
        CommandGenerator {
            stdout,
            stderr,
            stdout_buffer: Vec::new(),
            stderr_buffer: Vec::new(),
            get_stdout,
            get_stderr,
            get_combined,
        }
    }

    /// Read a line from standard output (non-blocking).
    fn stdout_read_line_nb(&mut self) -> Option<String> {
        let mut index = self.stdout_buffer.iter().position(|&r| r == b'\n');

        if index.is_none() && !self.stdout.is_eof() {
            let _res = self.stdout.read_available(&mut self.stdout_buffer);
            index = self.stdout_buffer.iter().position(|&r| r == b'\n');
        }
        match index {
            Some(n) => {
                /* todo: may not work if newline falls within
                 * a multibyte Unicode character?  Not sure if this
                 * is possible, though. */
                let new_buf: Vec<u8> = (&mut self.stdout_buffer).drain(0..(n + 1)).collect();
                let new_str = std::str::from_utf8(&new_buf).unwrap();
                Some(new_str.to_string())
            }
            _ => {
                if !self.stdout_buffer.is_empty() && self.stdout.is_eof() {
                    let new_buf: Vec<u8> = (&mut self.stdout_buffer).drain(..).collect();
                    let new_str = std::str::from_utf8(&new_buf).unwrap();
                    Some(new_str.to_string())
                } else {
                    None
                }
            }
        }
    }

    /// Read a line from standard error (non-blocking).
    fn stderr_read_line_nb(&mut self) -> Option<String> {
        let mut index = self.stderr_buffer.iter().position(|&r| r == b'\n');

        if index.is_none() && !self.stderr.is_eof() {
            let _res = self.stderr.read_available(&mut self.stderr_buffer);
            index = self.stderr_buffer.iter().position(|&r| r == b'\n');
        }
        match index {
            Some(n) => {
                /* todo: may not work if newline falls within
                 * a multibyte Unicode character?  Not sure if this
                 * is possible, though. */
                let new_buf: Vec<u8> = (&mut self.stderr_buffer).drain(0..(n + 1)).collect();
                let new_str = std::str::from_utf8(&new_buf).unwrap();
                Some(new_str.to_string())
            }
            _ => {
                if !self.stderr_buffer.is_empty() && self.stderr.is_eof() {
                    let new_buf: Vec<u8> = (&mut self.stderr_buffer).drain(..).collect();
                    let new_str = std::str::from_utf8(&new_buf).unwrap();
                    Some(new_str.to_string())
                } else {
                    None
                }
            }
        }
    }

    /// Read a line from standard output or standard error.  This
    /// blocks until one returns a line.
    pub fn read_line(&mut self) -> Option<String> {
        let mut s = None;
        while s.is_none() {
            if self.get_stdout {
                s = self.stdout_read_line_nb();
                if s.is_some() {
                    return s;
                } else if self.stdout.is_eof() && (!self.get_stderr || self.stderr.is_eof()) {
                    return None;
                }
            }
            if self.get_stderr {
                s = self.stderr_read_line_nb();
                if s.is_some() {
                    return s;
                } else if self.stderr.is_eof() && (!self.get_stdout || self.stdout.is_eof()) {
                    return None;
                }
            }
        }
        None
    }

    /// Read a line from standard output or standard error, and return
    /// an identifier for the stream and the string.  This blocks
    /// until one returns a line.
    pub fn read_line_combined(&mut self) -> Option<(i32, String)> {
        let mut s = None;
        while s.is_none() {
            s = self.stdout_read_line_nb();
            match s {
                Some(ss) => {
                    return Some((1, ss));
                }
                _ => {
                    if self.stdout.is_eof() && self.stderr.is_eof() {
                        return None;
                    }
                }
            }

            s = self.stderr_read_line_nb();
            match s {
                Some(ss) => {
                    return Some((2, ss));
                }
                _ => {
                    if self.stderr.is_eof() && self.stdout.is_eof() {
                        return None;
                    }
                }
            }
        }
        None
    }
}

/// The core value type used by the compiler and VM.
#[derive(Clone)]
pub enum Value {
    /// Used to indicate that a generator is exhausted.
    Null,
    /// Boolean.
    Bool(bool),
    /// 32-bit integer.
    Int(i32),
    /// Unbounded integer.
    BigInt(num_bigint::BigInt),
    /// Floating-point number.
    Float(f64),
    /// String.  The second part here is the regex object that
    /// corresponds to the string, which is generated and cached
    /// when the string is used as a regex.
    String(Rc<RefCell<StringTriple>>),
    /// An external command (wrapped in curly brackets), where the
    /// output is captured.
    Command(Rc<String>, Rc<HashSet<char>>),
    /// An external command (begins with $), where the output is not
    /// captured.
    CommandUncaptured(Rc<String>),
    /// A list.
    List(Rc<RefCell<VecDeque<Value>>>),
    /// A hash.
    Hash(Rc<RefCell<IndexMap<String, Value>>>),
    /// A set.  The stringification of the value is used as the map
    /// key, and the set may only contain values of a single type.
    /// (Not terribly efficient, but can be made decent later without
    /// affecting the language interface.)
    Set(Rc<RefCell<IndexMap<String, Value>>>),
    /// An anonymous function (includes reference to local variable
    /// stack).
    AnonymousFunction(Rc<RefCell<Chunk>>, Rc<RefCell<Vec<Value>>>),
    /// A core function.  See SIMPLE_FORMS in the VM.
    CoreFunction(fn(&mut VM) -> i32),
    /// A named function.
    NamedFunction(Rc<RefCell<Chunk>>),
    /// A generator constructed by way of a generator function.
    Generator(Rc<RefCell<GeneratorObject>>),
    /// A generator for getting the output of a Command.
    CommandGenerator(Rc<RefCell<CommandGenerator>>),
    /// A generator over the keys of a hash.
    KeysGenerator(Rc<RefCell<HashWithIndex>>),
    /// A generator over the values of a hash.
    ValuesGenerator(Rc<RefCell<HashWithIndex>>),
    /// A generator over key-value pairs (lists) of a hash.
    EachGenerator(Rc<RefCell<HashWithIndex>>),
    /// A file reader value.
    FileReader(Rc<RefCell<BufReader<File>>>),
    /// A file writer value.
    FileWriter(Rc<RefCell<BufWriter<File>>>),
    /// A directory handle.
    DirectoryHandle(Rc<RefCell<ReadDir>>),
    /// A datetime with a named timezone.
    DateTimeNT(DateTime<chrono_tz::Tz>),
    /// A datetime with an offset timezone.
    DateTimeOT(DateTime<FixedOffset>),
    /// An IPv4 address/prefix object.
    Ipv4(Ipv4Net),
    /// An IPv6 address/prefix object.
    Ipv6(Ipv6Net),
    /// An IPv4 range object (arbitrary start/end addresses).
    Ipv4Range(Ipv4Range),
    /// An IPv6 range object (arbitrary start/end addresses).
    Ipv6Range(Ipv6Range),
    /// An IP set (IPv4 and IPv6 together).
    IpSet(Rc<RefCell<IpSet>>),
    /// Multiple generators combined together.
    MultiGenerator(Rc<RefCell<VecDeque<Value>>>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Null => {
                write!(f, "Null")
            }
            Value::Int(i) => {
                write!(f, "{}", i)
            }
            Value::BigInt(i) => {
                write!(f, "{}", i)
            }
            Value::Float(i) => {
                write!(f, "{}", i)
            }
            Value::Bool(b) => {
                write!(f, "{}", b)
            }
            Value::String(s) => {
                let ss = &s.borrow().string;
                write!(f, "\"{}\"", ss)
            }
            Value::Command(s, _) => {
                write!(f, "Command \"{}\"", s)
            }
            Value::CommandUncaptured(s) => {
                write!(f, "CommandUncaptured \"{}\"", s)
            }
            Value::List(ls) => {
                write!(f, "{:?}", ls)
            }
            Value::Hash(hs) => {
                write!(f, "{:?}", hs)
            }
            Value::Set(st) => {
                write!(f, "{:?}", st)
            }
            Value::AnonymousFunction(_, _) => {
                write!(f, "((Function))")
            }
            Value::CoreFunction(_) => {
                write!(f, "((CoreFunction))")
            }
            Value::NamedFunction(_) => {
                write!(f, "((NamedFunction))")
            }
            Value::Generator(_) => {
                write!(f, "((Generator))")
            }
            Value::CommandGenerator(_) => {
                write!(f, "((CommandGenerator))")
            }
            Value::KeysGenerator(_) => {
                write!(f, "((KeysGenerator))")
            }
            Value::ValuesGenerator(_) => {
                write!(f, "((ValuesGenerator))")
            }
            Value::EachGenerator(_) => {
                write!(f, "((EachGenerator))")
            }
            Value::FileReader(_) => {
                write!(f, "((FileReader))")
            }
            Value::FileWriter(_) => {
                write!(f, "((FileWriter))")
            }
            Value::DirectoryHandle(_) => {
                write!(f, "((DirectoryHandle))")
            }
            Value::DateTimeNT(_) => {
                write!(f, "((DateTimeNT))")
            }
            Value::DateTimeOT(_) => {
                write!(f, "((DateTimeOT))")
            }
            Value::Ipv4(_) => {
                write!(f, "((IPv4))")
            }
            Value::Ipv4Range(_) => {
                write!(f, "((IPv4Range))")
            }
            Value::Ipv6(_) => {
                write!(f, "((IPv6))")
            }
            Value::Ipv6Range(_) => {
                write!(f, "((IPv6))")
            }
            Value::IpSet(_) => {
                write!(f, "((IpSet))")
            }
            Value::MultiGenerator(_) => {
                write!(f, "((MultiGenerator))")
            }
        }
    }
}

/// An enum for the Value types that can be serialised and
/// deserialised (i.e. those that can be stored as constants in a
/// chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueSD {
    Null,
    Bool(bool),
    Int(i32),
    Float(f64),
    BigInt(String),
    String(String, String),
    Command(String, HashSet<char>),
    CommandUncaptured(String),
}

/// Takes a chunk, an instruction index, and an error message as its
/// arguments.  Prints the error message, including filename, line number
/// and column number elements (if applicable).
pub fn print_error(chunk: Rc<RefCell<Chunk>>, i: usize, error: &str) {
    let point = chunk.borrow().get_point(i);
    let name = &chunk.borrow().name;
    let error_start = if name == "(main)" {
        String::new()
    } else {
        format!("{}:", name)
    };
    match point {
        Some((line, col)) => {
            eprintln!("{}{}:{}: {}", error_start, line, col, error);
        }
        _ => {
            eprintln!("{}{}", error_start, error);
        }
    }
}

impl Chunk {
    /// Construct a standard (non-generator) chunk.
    pub fn new_standard(name: String) -> Chunk {
        Chunk {
            name,
            data: Vec::new(),
            points: Vec::new(),
            constants: Vec::new(),
            functions: HashMap::new(),
            is_generator: false,
            has_vars: true,
            arg_count: 0,
            req_arg_count: 0,
            nested: false,
            scope_depth: 0,
            constant_values: Vec::new(),
        }
    }

    /// Construct a generator chunk.
    pub fn new_generator(name: String, arg_count: i32, req_arg_count: i32) -> Chunk {
        Chunk {
            name,
            data: Vec::new(),
            points: Vec::new(),
            constants: Vec::new(),
            functions: HashMap::new(),
            is_generator: true,
            has_vars: true,
            arg_count,
            req_arg_count,
            nested: false,
            scope_depth: 0,
            constant_values: Vec::new(),
        }
    }

    /// Add a constant to the current chunk, and return its index in
    /// the constants list (for later calls to `get_constant`).
    pub fn add_constant(&mut self, value_rr: Value) -> i32 {
        let value_sd = match value_rr {
            Value::Null => ValueSD::Null,
            Value::Int(n) => ValueSD::Int(n),
            Value::Float(n) => ValueSD::Float(n),
            Value::BigInt(n) => ValueSD::BigInt(n.to_str_radix(10)),
            Value::String(st) => ValueSD::String(
                st.borrow().string.to_string(),
                st.borrow().escaped_string.to_string(),
            ),
            Value::Command(s, params) => ValueSD::Command(s.to_string(), (*params).clone()),
            Value::CommandUncaptured(s) => ValueSD::CommandUncaptured(s.to_string()),
            Value::Bool(b) => ValueSD::Bool(b),
            _ => {
                eprintln!("constant type cannot be added to chunk! {:?}", value_rr);
                std::process::abort();
            }
        };
        self.constants.push(value_sd);
        (self.constants.len() - 1) as i32
    }

    /// Get a constant from the current chunk.
    pub fn get_constant(&self, i: i32) -> Value {
        let value_sd = &self.constants[i as usize];
        match value_sd {
            ValueSD::Null => Value::Null,
            ValueSD::Bool(b) => Value::Bool(*b),
            ValueSD::Int(n) => Value::Int(*n),
            ValueSD::Float(n) => Value::Float(*n),
            ValueSD::BigInt(n) => {
                let nn = n.parse::<num_bigint::BigInt>().unwrap();
                Value::BigInt(nn)
            }
            ValueSD::String(st1, st2) => {
                let st = StringTriple::new_with_escaped(st1.to_string(), st2.to_string(), None);
                Value::String(Rc::new(RefCell::new(st)))
            }
            ValueSD::Command(s, params) => {
                Value::Command(Rc::new(s.to_string()), Rc::new((*params).clone()))
            }
            ValueSD::CommandUncaptured(s) => Value::CommandUncaptured(Rc::new(s.to_string())),
        }
    }

    /// Get a constant value from the current chunk.  If the relevant
    /// constant value has not been initialised, this will return
    /// Value::Null.
    pub fn get_constant_value(&self, i: i32) -> Value {
        let value = self.constant_values.get(i as usize);
        match value {
            Some(v) => v.clone(),
            _ => Value::Null,
        }
    }

    /// Get a constant int value from the current chunk.
    pub fn get_constant_int(&self, i: i32) -> i32 {
        let value_sd = &self.constants[i as usize];
        match *value_sd {
            ValueSD::Int(n) => n,
            _ => 0,
        }
    }

    /// Check whether the chunk has a constant int value at the
    /// specified index.
    pub fn has_constant_int(&self, i: i32) -> bool {
        let value_sd = &self.constants[i as usize];
        matches!(*value_sd, ValueSD::Int(_))
    }

    /// Add an opcode to the current chunk's data.
    pub fn add_opcode(&mut self, opcode: OpCode) {
        self.data.push(opcode as u8);
    }

    /// Get the last opcode from the current chunk's data.
    pub fn get_last_opcode(&self) -> OpCode {
        return to_opcode(*self.data.last().unwrap());
    }

    /// Get the second-last opcode from the current chunk's data.
    /// Defaults to `OpCode::Call`, if the chunk does not have at
    /// least two opcodes.  Used for adding implicit call opcodes, if
    /// required.
    pub fn get_second_last_opcode(&self) -> OpCode {
        if self.data.len() < 2 {
            return OpCode::Call;
        }
        return to_opcode(*self.data.get(self.data.len() - 2).unwrap());
    }

    /// Get the third-last opcode from the current chunk's data.
    pub fn get_third_last_opcode(&self) -> OpCode {
        if self.data.len() < 3 {
            return OpCode::Call;
        }
        return to_opcode(*self.data.get(self.data.len() - 3).unwrap());
    }

    /// Get the fourth-last opcode from the current chunk's data.
    pub fn get_fourth_last_opcode(&self) -> OpCode {
        if self.data.len() < 4 {
            return OpCode::Call;
        }
        return to_opcode(*self.data.get(self.data.len() - 4).unwrap());
    }

    /// Set the second-last opcode for the current chunk's data.
    pub fn set_second_last_opcode(&mut self, opcode: OpCode) {
        let len = self.data.len();
        if let Some(el) = self.data.get_mut(len - 2) {
            *el = opcode as u8;
        }
    }

    /// Set the third-last opcode for the current chunk's data.
    pub fn set_third_last_opcode(&mut self, opcode: OpCode) {
        let len = self.data.len();
        if let Some(el) = self.data.get_mut(len - 3) {
            *el = opcode as u8;
        }
    }

    /// Set the fourth-last opcode for the current chunk's data.
    pub fn set_fourth_last_opcode(&mut self, opcode: OpCode) {
        let len = self.data.len();
        if let Some(el) = self.data.get_mut(len - 4) {
            *el = opcode as u8;
        }
    }

    /// Set the last opcode for the current chunk's data.
    pub fn set_last_opcode(&mut self, opcode: OpCode) {
        let len = self.data.len();
        if let Some(el) = self.data.get_mut(len - 1) {
            *el = opcode as u8;
        }
    }

    /// Add a raw byte to the current chunk's data.
    pub fn add_byte(&mut self, byte: u8) {
        self.data.push(byte);
    }

    /// Remove the last byte from the current chunk's data.
    pub fn pop_byte(&mut self) {
        self.data.pop();
    }

    /// Get the last byte from the current chunk's data.
    pub fn get_last_byte(&self) -> u8 {
        return *self.data.last().unwrap();
    }

    /// Get the second-last byte from the current chunk's data.
    pub fn get_second_last_byte(&self) -> u8 {
        if self.data.len() < 2 {
            return 0;
        }
        return *self.data.get(self.data.len() - 2).unwrap();
    }

    /// Get the third-last byte from the current chunk's data.
    pub fn get_third_last_byte(&self) -> u8 {
        if self.data.len() < 3 {
            return 0;
        }
        return *self.data.get(self.data.len() - 3).unwrap();
    }

    /// Set the last byte for the current chunk's data.
    pub fn set_last_byte(&mut self, byte: u8) {
        let len = self.data.len();
        if let Some(el) = self.data.get_mut(len - 1) {
            *el = byte;
        }
    }

    /// Set the second-last byte for the current chunk's data.
    pub fn set_second_last_byte(&mut self, byte: u8) {
        let len = self.data.len();
        if let Some(el) = self.data.get_mut(len - 2) {
            *el = byte;
        }
    }

    /// Set the third-last byte for the current chunk's data.
    pub fn set_third_last_byte(&mut self, byte: u8) {
        let len = self.data.len();
        if let Some(el) = self.data.get_mut(len - 3) {
            *el = byte;
        }
    }

    /// Get the chunk's most recently-added constant.
    pub fn get_last_constant(&mut self) -> Value {
        self.get_constant((self.constants.len() - 1).try_into().unwrap())
    }

    /// Set the line and column number data for the most
    /// recently-added opcode/byte.  If any of the preceding
    /// opcodes/bytes do not have point data, set the point data for
    /// those opcodes/bytes by using the most recently-added point
    /// data (putting aside the current call), too.
    pub fn set_next_point(&mut self, line_number: u32, column_number: u32) {
        let data_len = self.data.len();
        let points_len = self.points.len();
        let mut prev_line_number = 0;
        let mut prev_column_number = 0;
        if points_len > 0 {
            let last_point = self.points.get(points_len - 1).unwrap();
            let (prev_line_number_p, prev_column_number_p) = last_point;
            prev_line_number = *prev_line_number_p;
            prev_column_number = *prev_column_number_p;
        }
        while self.points.len() < data_len {
            self.points.push((prev_line_number, prev_column_number));
        }
        self.points.insert(data_len, (line_number, column_number));
    }

    /// Get the point data for the given index.
    pub fn get_point(&self, i: usize) -> Option<(u32, u32)> {
        let point = self.points.get(i);
        match point {
            Some((0, 0)) => None,
            Some((_, _)) => Some(*(point.unwrap())),
            _ => None,
        }
    }

    /// Reset the values for a previous point.  Required where
    /// existing data is being replaced/adjusted during compilation.
    pub fn set_previous_point(&mut self, i: usize, line_number: u32, column_number: u32) {
        let point = self.points.get_mut(i);
        match point {
            Some((ref mut a, ref mut b)) => {
                *a = line_number;
                *b = column_number;
            }
            _ => {
                eprintln!("point not found!");
                std::process::abort();
            }
        }
    }

    /// Print the disassembly for the current chunk to standard
    /// output.
    pub fn disassemble(&self, name: &str) {
        println!("== {} ==", name);

        let mut i = 0;
        while i < self.data.len() {
            let opcode = to_opcode(self.data[i]);
            print!("{:^4} ", i);
            match opcode {
                OpCode::Clone => {
                    println!("OP_CLONE");
                }
                OpCode::Constant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_CONSTANT {:?}", value);
                }
                OpCode::AddConstant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_ADDCONSTANT {:?}", value);
                }
                OpCode::SubtractConstant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_SUBTRACTCONSTANT {:?}", value);
                }
                OpCode::DivideConstant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_DIVIDECONSTANT {:?}", value);
                }
                OpCode::MultiplyConstant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_MULTIPLYCONSTANT {:?}", value);
                }
                OpCode::EqConstant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_EQCONSTANT {:?}", value);
                }
                OpCode::Add => {
                    println!("OP_ADD");
                }
                OpCode::Subtract => {
                    println!("OP_SUBTRACT");
                }
                OpCode::Multiply => {
                    println!("OP_MULTIPLY");
                }
                OpCode::Divide => {
                    println!("OP_DIVIDE");
                }
                OpCode::EndFn => {
                    println!("OP_ENDFN");
                }
                OpCode::Call => {
                    println!("OP_CALL");
                }
                OpCode::CallImplicit => {
                    println!("OP_CALLIMPLICIT");
                }
                OpCode::Function => {
                    println!("OP_FUNCTION");
                }
                OpCode::Var => {
                    println!("OP_VAR");
                }
                OpCode::SetVar => {
                    println!("OP_SETVAR");
                }
                OpCode::GetVar => {
                    println!("OP_GETVAR");
                }
                OpCode::SetLocalVar => {
                    i += 1;
                    let var_i = self.data[i];
                    println!("OP_SETLOCALVAR {}", var_i);
                }
                OpCode::GetLocalVar => {
                    i += 1;
                    let var_i = self.data[i];
                    println!("OP_GETLOCALVAR {}", var_i);
                }
                OpCode::GLVShift => {
                    i += 1;
                    let var_i = self.data[i];
                    println!("OP_GLVSHIFT {}", var_i);
                }
                OpCode::GLVCall => {
                    i += 1;
                    let var_i = self.data[i];
                    println!("OP_GLVCALL {}", var_i);
                }
                OpCode::PopLocalVar => {
                    println!("OP_POPLOCALVAR");
                }
                OpCode::Jump => {
                    i += 1;
                    let i1: usize = self.data[i].try_into().unwrap();
                    i += 1;
                    let i2: usize = self.data[i].try_into().unwrap();
                    let jump_i: usize = (i1 << 8) | i2;
                    println!("OP_JUMP {:?}", jump_i);
                }
                OpCode::JumpR => {
                    i += 1;
                    let i1: usize = self.data[i].try_into().unwrap();
                    i += 1;
                    let i2: usize = self.data[i].try_into().unwrap();
                    let jump_i: usize = (i1 << 8) | i2;
                    println!("OP_JUMPR {:?}", jump_i);
                }
                OpCode::JumpNe => {
                    i += 1;
                    let i1: usize = self.data[i].try_into().unwrap();
                    i += 1;
                    let i2: usize = self.data[i].try_into().unwrap();
                    let jump_i: usize = (i1 << 8) | i2;
                    println!("OP_JUMPNE {:?}", jump_i);
                }
                OpCode::JumpNeR => {
                    i += 1;
                    let i1: usize = self.data[i].try_into().unwrap();
                    i += 1;
                    let i2: usize = self.data[i].try_into().unwrap();
                    let jump_i: usize = (i1 << 8) | i2;
                    println!("OP_JUMPNER {:?}", jump_i);
                }
                OpCode::JumpNeREqC => {
                    i += 1;
                    let i1: usize = self.data[i].try_into().unwrap();
                    i += 1;
                    let i2: usize = self.data[i].try_into().unwrap();
                    let jump_i: usize = (i1 << 8) | i2;

                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);

                    println!("OP_JUMPNEREQC {:?} {:?}", jump_i, value);
                }
                OpCode::Cmp => {
                    println!("OP_CMP");
                }
                OpCode::Eq => {
                    println!("OP_EQ");
                }
                OpCode::Gt => {
                    println!("OP_GT");
                }
                OpCode::Lt => {
                    println!("OP_LT");
                }
                OpCode::Print => {
                    println!("OP_PRINT");
                }
                OpCode::Dup => {
                    println!("OP_DUP");
                }
                OpCode::Swap => {
                    println!("OP_SWAP");
                }
                OpCode::Drop => {
                    println!("OP_DROP");
                }
                OpCode::Rot => {
                    println!("OP_ROT");
                }
                OpCode::Over => {
                    println!("OP_OVER");
                }
                OpCode::Depth => {
                    println!("OP_DEPTH");
                }
                OpCode::Clear => {
                    println!("OP_CLEAR");
                }
                OpCode::StartList => {
                    println!("OP_STARTLIST");
                }
                OpCode::EndList => {
                    println!("OP_ENDLIST");
                }
                OpCode::StartHash => {
                    println!("OP_STARTHASH");
                }
                OpCode::StartSet => {
                    println!("OP_STARTSET");
                }
                OpCode::Shift => {
                    println!("OP_SHIFT");
                }
                OpCode::Yield => {
                    println!("OP_YIELD");
                }
                OpCode::IsNull => {
                    println!("OP_ISNULL");
                }
                OpCode::IsList => {
                    println!("OP_ISLIST");
                }
                OpCode::IsCallable => {
                    println!("OP_ISCALLABLE");
                }
                OpCode::IsShiftable => {
                    println!("OP_ISSHIFTABLE");
                }
                OpCode::Open => {
                    println!("OP_OPEN");
                }
                OpCode::Readline => {
                    println!("OP_READLINE");
                }
                OpCode::Error => {
                    println!("OP_ERROR");
                }
                OpCode::Return => {
                    println!("OP_RETURN");
                }
                OpCode::Str => {
                    println!("OP_STR");
                }
                OpCode::Int => {
                    println!("OP_INT");
                }
                OpCode::Flt => {
                    println!("OP_FLT")
                }
                OpCode::Rand => {
                    println!("OP_RAND")
                }
                OpCode::Push => {
                    println!("OP_PUSH")
                }
                OpCode::Pop => {
                    println!("OP_POP")
                }
                OpCode::DupIsNull => {
                    println!("OP_DUPISNULL")
                }
                OpCode::ToggleMode => {
                    println!("OP_TOGGLEMODE")
                }
                OpCode::PrintStack => {
                    println!("OP_PRINTSTACK")
                }
                OpCode::ToFunction => {
                    println!("OP_TOFUNCTION")
                }
                OpCode::Import => {
                    println!("OP_IMPORT")
                }
                OpCode::CallConstant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_CALLCONSTANT {:?}", value);
                }
                OpCode::CallImplicitConstant => {
                    i += 1;
                    let i_upper = self.data[i];
                    i += 1;
                    let i_lower = self.data[i];
                    let constant_i = (((i_upper as u16) << 8) & 0xFF00) | (i_lower as u16);
                    let value = self.get_constant(constant_i as i32);
                    println!("OP_CALLIMPLICITCONSTANT {:?}", value);
                }
                OpCode::Bool => {
                    println!("OP_BOOL");
                }
                OpCode::IsBool => {
                    println!("OP_ISBOOL");
                }
                OpCode::IsInt => {
                    println!("OP_ISINT");
                }
                OpCode::IsBigInt => {
                    println!("OP_ISBIGINT");
                }
                OpCode::IsStr => {
                    println!("OP_ISSTR");
                }
                OpCode::IsFlt => {
                    println!("OP_ISFLT");
                }
                OpCode::BigInt => {
                    println!("OP_BIGINT");
                }
                OpCode::Unknown => {
                    println!("(Unknown)");
                }
            }
            i += 1;
        }

        for (k, v) in self.functions.iter() {
            println!("== {}.{} ==", name, k);
            v.borrow().disassemble(k);
        }
    }
}

/// A macro for converting a value into a string, that avoids any
/// copies or similar if the argument is already a string.
macro_rules! to_str {
    ($val:expr, $var:expr) => {
        let lib_str_s;
        let lib_str_b;
        let lib_str_str;
        let lib_str_bk: Option<String>;
        $var = match $val {
            Value::String(st) => {
                lib_str_s = st;
                lib_str_b = lib_str_s.borrow();
                Some(&lib_str_b.string)
            }
            _ => {
                lib_str_bk = $val.to_string();
                match lib_str_bk {
                    Some(s) => {
                        lib_str_str = s;
                        Some(&lib_str_str)
                    }
                    _ => None,
                }
            }
        }
    };
}

impl Value {
    /// Convert the current value into a string.  Not intended for use
    /// with Value::String.
    pub fn to_string(&self) -> Option<String> {
        match self {
            Value::String(_) => {
                eprintln!("to_string should not be called with Value::String!");
                std::process::abort();
            }
            Value::Int(n) => {
                let s = format!("{}", n);
                Some(s)
            }
            Value::BigInt(n) => {
                let s = format!("{}", n);
                Some(s)
            }
            Value::Float(f) => {
                let s = format!("{}", f);
                Some(s)
            }
            Value::Ipv4(ipv4net) => {
                let prefix_len = ipv4net.prefix_len();
                if prefix_len == 32 {
                    let ip_str = format!("{}", ipv4net);
                    let ip_str_no_len =
                        ip_str.chars().take_while(|&c| c != '/').collect::<String>();
                    Some(ip_str_no_len)
                } else {
                    let s = format!("{}", ipv4net);
                    Some(s)
                }
            }
            Value::Ipv6(ipv6net) => {
                let prefix_len = ipv6net.prefix_len();
                if prefix_len == 128 {
                    let s = format!("{}", ipv6net.network());
                    Some(s)
                } else {
                    let s = format!("{}/{}", ipv6net.network(), ipv6net.prefix_len());
                    Some(s)
                }
            }
            Value::Ipv4Range(ipv4range) => {
                let s = format!("{}-{}", ipv4range.s, ipv4range.e);
                Some(s)
            }
            Value::Ipv6Range(ipv6range) => {
                let s = format!("{}-{}", ipv6range.s, ipv6range.e);
                Some(s)
            }
            Value::IpSet(ipset) => {
                let ipv4range = &ipset.borrow().ipv4;
                let ipv6range = &ipset.borrow().ipv6;
                let mut lst = Vec::new();
                let mut ipv4lst = ipv4range.iter().collect::<Vec<Ipv4Net>>();
                ipv4lst.sort_by_key(|a| a.network());
                for ipv4net in ipv4lst.iter() {
                    let prefix_len = ipv4net.prefix_len();
                    if prefix_len == 32 {
                        let ip_str = format!("{}", ipv4net);
                        let ip_str_no_len =
                            ip_str.chars().take_while(|&c| c != '/').collect::<String>();
                        lst.push(ip_str_no_len);
                    } else {
                        let ip_str = format!("{}", ipv4net);
                        lst.push(ip_str);
                    }
                }
                let mut ipv6lst = ipv6range.iter().collect::<Vec<Ipv6Net>>();
                ipv6lst.sort_by_key(|a| a.network());
                for ipv6net in ipv6lst.iter() {
                    let prefix_len = ipv6net.prefix_len();
                    if prefix_len == 128 {
                        let ip_str = format!("{}", ipv6net.network());
                        lst.push(ip_str);
                    } else {
                        let ip_str = format!("{}/{}", ipv6net.network(), ipv6net.prefix_len());
                        lst.push(ip_str);
                    }
                }
                let s = lst.join(",");
                Some(s)
            }
            Value::Null => Some("".to_string()),
            _ => None,
        }
    }

    /// Convert the current value into an i32.  If the value is not
    /// representable as an i32, the result will be None.
    pub fn to_int(&self) -> Option<i32> {
        match self {
            Value::Int(n) => Some(*n),
            Value::BigInt(n) => n.to_i32(),
            Value::Float(f) => Some(*f as i32),
            Value::String(st) => {
                let s = &st.borrow().string;
                let n_r = s.parse::<i32>();
                match n_r {
                    Ok(n) => Some(n),
                    _ => None,
                }
            }
            Value::Null => Some(0),
            _ => None,
        }
    }

    /// Convert the current value into a bigint.  If the value is not
    /// representable as a bigint, the result will be None.
    pub fn to_bigint(&self) -> Option<BigInt> {
        match self {
            Value::Int(n) => Some(BigInt::from_i32(*n).unwrap()),
            Value::BigInt(n) => Some(n.clone()),
            Value::Float(f) => Some(BigInt::from_i32(*f as i32).unwrap()),
            Value::String(st) => {
                let s = &st.borrow().string;
                let n_r = s.to_string().parse::<num_bigint::BigInt>();
                match n_r {
                    Ok(n) => Some(n),
                    _ => None,
                }
            }
            Value::Null => Some(BigInt::from_i32(0).unwrap()),
            _ => None,
        }
    }

    /// Convert the current value into a floating-point number (f64).
    /// If the value is not representable in that type, the result
    /// will be None.
    pub fn to_float(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::BigInt(n) => Some(n.to_f64().unwrap()),
            Value::Float(f) => Some(*f),
            Value::String(st) => {
                let s = &st.borrow().string;
                let n_r = s.parse::<f64>();
                match n_r {
                    Ok(n) => Some(n),
                    _ => None,
                }
            }
            Value::Null => Some(0.0),
            _ => None,
        }
    }

    pub fn to_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(0) => false,
            Value::Float(n) => *n != 0.0,
            Value::String(st) => {
                let ss = &st.borrow().string;
                !ss.is_empty() && ss != "0" && ss != "0.0"
            }
            Value::BigInt(n) => *n == Zero::zero(),
            Value::Null => false,
            _ => true,
        }
    }

    pub fn value_clone(&self) -> Value {
        match self {
            Value::Null => self.clone(),
            Value::Bool(_) => self.clone(),
            Value::Int(_) => self.clone(),
            Value::BigInt(_) => self.clone(),
            Value::Float(_) => self.clone(),
            Value::String(_) => self.clone(),
            Value::Command(_, _) => self.clone(),
            Value::CommandUncaptured(_) => self.clone(),
            Value::List(lst) => {
                let cloned_lst = lst.borrow().iter().map(|v| v.value_clone()).collect();
                Value::List(Rc::new(RefCell::new(cloned_lst)))
            }
            Value::Hash(hsh) => {
                let mut cloned_hsh = IndexMap::new();
                for (k, v) in hsh.borrow().iter() {
                    cloned_hsh.insert(k.clone(), v.value_clone());
                }
                Value::Hash(Rc::new(RefCell::new(cloned_hsh)))
            }
            Value::Set(hsh) => {
                let mut cloned_hsh = IndexMap::new();
                for (k, v) in hsh.borrow().iter() {
                    cloned_hsh.insert(k.clone(), v.value_clone());
                }
                Value::Set(Rc::new(RefCell::new(cloned_hsh)))
            }
            Value::AnonymousFunction(_, _) => self.clone(),
            Value::CoreFunction(_) => self.clone(),
            Value::NamedFunction(_) => self.clone(),
            Value::Generator(gen_ref) => {
                let gen = gen_ref.borrow();
                let local_vars_stack = gen.local_vars_stack.clone();
                let index = gen.index;
                let chunk = gen.chunk.clone();
                let call_stack_chunks = gen.call_stack_chunks.clone();
                let gen_args = gen.gen_args.clone();
                let new_gen = GeneratorObject::new(
                    Rc::new(RefCell::new(local_vars_stack.borrow().clone())),
                    index,
                    chunk,
                    call_stack_chunks,
                    gen_args,
                );
                Value::Generator(Rc::new(RefCell::new(new_gen)))
            }
            Value::CommandGenerator(_) => self.clone(),
            Value::KeysGenerator(keys_gen_ref) => {
                Value::KeysGenerator(Rc::new(RefCell::new(keys_gen_ref.borrow().clone())))
            }
            Value::ValuesGenerator(values_gen_ref) => {
                Value::ValuesGenerator(Rc::new(RefCell::new(values_gen_ref.borrow().clone())))
            }
            Value::EachGenerator(each_gen_ref) => {
                Value::EachGenerator(Rc::new(RefCell::new(each_gen_ref.borrow().clone())))
            }
            Value::FileReader(_) => self.clone(),
            Value::FileWriter(_) => self.clone(),
            Value::DirectoryHandle(_) => self.clone(),
            Value::DateTimeNT(_) => self.clone(),
            Value::DateTimeOT(_) => self.clone(),
            Value::Ipv4(_) => self.clone(),
            Value::Ipv4Range(_) => self.clone(),
            Value::Ipv6(_) => self.clone(),
            Value::Ipv6Range(_) => self.clone(),
            Value::IpSet(_) => self.clone(),
            Value::MultiGenerator(_) => self.clone(),
        }
    }

    pub fn variants_equal(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(..), Value::Bool(..)) => true,
            (Value::Int(..), Value::Int(..)) => true,
            (Value::BigInt(..), Value::BigInt(..)) => true,
            (Value::Float(..), Value::Float(..)) => true,
            (Value::String(..), Value::String(..)) => true,
            (Value::Command(..), Value::Command(..)) => true,
            (Value::CommandUncaptured(..), Value::CommandUncaptured(..)) => true,
            (Value::List(..), Value::List(..)) => true,
            (Value::Hash(..), Value::Hash(..)) => true,
            (Value::Set(..), Value::Set(..)) => true,
            (Value::AnonymousFunction(..), Value::AnonymousFunction(..)) => true,
            (Value::CoreFunction(..), Value::CoreFunction(..)) => true,
            (Value::NamedFunction(..), Value::NamedFunction(..)) => true,
            (Value::Generator(..), Value::Generator(..)) => true,
            (Value::CommandGenerator(..), Value::CommandGenerator(..)) => true,
            (Value::KeysGenerator(..), Value::KeysGenerator(..)) => true,
            (Value::ValuesGenerator(..), Value::ValuesGenerator(..)) => true,
            (Value::EachGenerator(..), Value::EachGenerator(..)) => true,
            (Value::FileReader(..), Value::FileReader(..)) => true,
            (Value::FileWriter(..), Value::FileWriter(..)) => true,
            (Value::DirectoryHandle(..), Value::DirectoryHandle(..)) => true,
            (Value::DateTimeNT(..), Value::DateTimeNT(..)) => true,
            (Value::DateTimeOT(..), Value::DateTimeOT(..)) => true,
            (Value::Ipv4(..), Value::Ipv4(..)) => true,
            (Value::Ipv6(..), Value::Ipv6(..)) => true,
            (Value::Ipv4Range(..), Value::Ipv4Range(..)) => true,
            (Value::Ipv6Range(..), Value::Ipv6Range(..)) => true,
            (Value::IpSet(..), Value::IpSet(..)) => true,
            (Value::MultiGenerator(..), Value::MultiGenerator(..)) => true,
            (..) => false,
        }
    }

    pub fn is_generator(&self) -> bool {
        matches!(
            self,
            Value::Generator(..)
                | Value::KeysGenerator(..)
                | Value::ValuesGenerator(..)
                | Value::EachGenerator(..)
                | Value::FileReader(..)
                | Value::DirectoryHandle(..)
                | Value::IpSet(..)
                | Value::MultiGenerator(..)
        )
    }

    pub fn type_string(&self) -> String {
        let s = match self {
            Value::Null => "null",
            Value::Bool(..) => "bool",
            Value::Int(..) => "int",
            Value::BigInt(..) => "bigint",
            Value::Float(..) => "float",
            Value::String(..) => "str",
            Value::Command(..) => "command",
            Value::CommandUncaptured(..) => "command",
            Value::List(..) => "list",
            Value::Hash(..) => "hash",
            Value::Set(..) => "set",
            Value::AnonymousFunction(..) => "anon-fn",
            Value::CoreFunction(..) => "core-fn",
            Value::NamedFunction(..) => "named-fn",
            Value::Generator(..) => "gen",
            Value::CommandGenerator(..) => "command-gen",
            Value::KeysGenerator(..) => "keys-gen",
            Value::ValuesGenerator(..) => "values-gen",
            Value::EachGenerator(..) => "each-gen",
            Value::FileReader(..) => "file-reader",
            Value::FileWriter(..) => "file-writer",
            Value::DirectoryHandle(..) => "dir-handle",
            Value::DateTimeNT(..) => "datetime",
            Value::DateTimeOT(..) => "datetime",
            Value::Ipv4(..) => "ip",
            Value::Ipv6(..) => "ip",
            Value::Ipv4Range(..) => "ip",
            Value::Ipv6Range(..) => "ip",
            Value::IpSet(..) => "ips",
            Value::MultiGenerator(..) => "multi-gen",
        };
        s.to_string()
    }
}
