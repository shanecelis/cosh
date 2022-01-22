use std::cell::RefCell;
use std::rc::Rc;

use chunk::{print_error, Chunk, Value};
use vm::*;

impl VM {
    /// Remove the top element from the stack.
    pub fn opcode_drop(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() == 0 {
            print_error(chunk, i, "drop requires one argument");
            return 0;
        }
        self.stack.pop().unwrap();
        return 1;
    }

    /// Remove all elements from the stack.
    #[allow(unused_variables)]
    pub fn opcode_clear(&mut self, chunk: &Chunk, i: usize) -> i32 {
        self.stack.clear();
        return 1;
    }

    /// Take the top element from the stack, duplicate it, and add it
    /// onto the stack.
    pub fn opcode_dup(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() == 0 {
            print_error(chunk, i, "dup requires one argument");
            return 0;
        }
        self.stack.push(self.stack.last().unwrap().clone());
        return 1;
    }

    /// Take the second element from the top from the stack, duplicate
    /// it, and add it onto the stack.
    pub fn opcode_over(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 2 {
            print_error(chunk, i, "over requires two arguments");
            return 0;
        }
        self.stack.push(self.stack[self.stack.len() - 2].clone());
        return 1;
    }

    /// Swap the top two elements from the stack.
    pub fn opcode_swap(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 2 {
            print_error(chunk, i, "swap requires two arguments");
            return 0;
        }
        let first_rr = self.stack.pop().unwrap();
        let second_rr = self.stack.pop().unwrap();
        self.stack.push(first_rr);
        self.stack.push(second_rr);
        return 1;
    }

    /// Rotate the top three elements from the stack: the top element
    /// becomes the second from top element, the second from top
    /// element becomes the third from top element, and the third from
    /// top element becomes the top element.
    pub fn opcode_rot(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 3 {
            print_error(chunk, i, "rot requires three arguments");
            return 0;
        }
        let first_rr = self.stack.pop().unwrap();
        let second_rr = self.stack.pop().unwrap();
        let third_rr = self.stack.pop().unwrap();
        self.stack.push(second_rr);
        self.stack.push(first_rr);
        self.stack.push(third_rr);
        return 1;
    }

    /// Push the current depth of the stack onto the stack.
    #[allow(unused_variables)]
    pub fn opcode_depth(&mut self, chunk: &Chunk, i: usize) -> i32 {
        self.stack
            .push(Rc::new(RefCell::new(Value::Int(self.stack.len() as i32))));
        return 1;
    }

    /// If the topmost element is a list, adds the length of that list
    /// onto the stack.  If the topmost element is a string, adds the
    /// length of that sting onto the stack.
    pub fn core_len(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 1 {
            print_error(chunk, i, "len requires one argument");
            return 0;
        }

        let lst_rr = self.stack.pop().unwrap();
        let lst_rrb = lst_rr.borrow();
        match &*lst_rrb {
            Value::List(lst) => {
                let len = lst.len();
                self.stack
                    .push(Rc::new(RefCell::new(Value::Int(len as i32))));
            }
            Value::String(s, _) => {
                let len = s.len();
                self.stack
                    .push(Rc::new(RefCell::new(Value::Int(len as i32))));
            }
            _ => {
                print_error(chunk, i, "len argument must be a list or a string");
                return 0;
            }
        }
        return 1;
    }

    /// Adds a boolean onto the stack indicating whether the topmost
    /// element is a null value.
    pub fn opcode_isnull(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 1 {
            print_error(chunk, i, "is-null requires one argument");
            return 0;
        }

        let i1_rr = self.stack.pop().unwrap();
        let i1_rrb = i1_rr.borrow();
        let is_null = match *i1_rrb {
            Value::Null => 1,
            _ => 0,
        };
        self.stack.push(Rc::new(RefCell::new(Value::Int(is_null))));
        return 1;
    }

    /// Adds a boolean onto the stack indicating whether the topmost
    /// element is a list.
    pub fn opcode_islist(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 1 {
            print_error(chunk, i, "is-list requires one argument");
            return 0;
        }

        let i1_rr = self.stack.pop().unwrap();
        let i1_rrb = i1_rr.borrow();
        let is_list = match *i1_rrb {
            Value::List(_) => 1,
            _ => 0,
        };
        self.stack.push(Rc::new(RefCell::new(Value::Int(is_list))));
        return 1;
    }

    /// Adds a boolean onto the stack indicating whether the topmost
    /// element can be called.  (In the case of a string, this doesn't
    /// currently check that the string name maps to a function or
    /// core form, though.)
    pub fn opcode_iscallable(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 1 {
            print_error(chunk, i, "is-callable requires one argument");
            return 0;
        }

        let i1_rr = self.stack.pop().unwrap();
        let i1_rrb = i1_rr.borrow();
        let is_callable = match *i1_rrb {
            Value::Function(_, _, _) => 1,
            /* This could be better. */
            Value::String(_, _) => 1,
            _ => 0,
        };
        self.stack.push(Rc::new(RefCell::new(Value::Int(is_callable))));
        return 1;
    }

    /// Convert a value into a string value.
    pub fn opcode_str(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 1 {
            print_error(chunk, i, "str requires one argument");
            return 0;
        }

        let value_rr = self.stack.pop().unwrap();
        let is_string;
        {
            let value_rrb = value_rr.borrow();
            match *value_rrb {
                Value::String(_, _) => {
                    is_string = true;
                }
                _ => {
                    let value_pre = value_rrb.to_string();
                    let value_opt = to_string_2(&value_pre);
                    match value_opt {
                        Some(s) => {
                            self.stack.push(Rc::new(RefCell::new(Value::String(s.to_string(), None))));
                            return 1;
                        }
                        _ => {
                            print_error(chunk, i, "unable to convert argument to string");
                            return 0;
                        }
                    }
                }
            }
        }
        if is_string {
            self.stack.push(value_rr);
        }
        return 1;
    }

    /// Convert a value into an integer/bigint value.
    pub fn opcode_int(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 1 {
            print_error(chunk, i, "int requires one argument");
            return 0;
        }

        let value_rr = self.stack.pop().unwrap();
        let is_int;
        {
            let value_rrb = value_rr.borrow();
            match *value_rrb {
                Value::Int(_) => {
                    is_int = true;
                }
                Value::BigInt(_) => {
                    is_int = true;
                }
                _ => {
                    let value_opt = value_rrb.to_int();
                    match value_opt {
                        Some(n) => {
                            self.stack.push(Rc::new(RefCell::new(Value::Int(n))));
                            return 1;
                        }
                        _ => {
                            let value_opt = value_rrb.to_bigint();
                            match value_opt {
                                Some(n) => {
                                    self.stack.push(Rc::new(RefCell::new(Value::BigInt(n))));
                                    return 1;
                                }
                                _ => {
                                    print_error(chunk, i, "unable to convert argument to int");
                                    return 0;
                                }
                            }
                        }
                    }
                }
            }
        }
        if is_int {
            self.stack.push(value_rr);
        }
        return 1;
    }

    /// Convert a value into a floating-point value.
    pub fn opcode_flt(&mut self, chunk: &Chunk, i: usize) -> i32 {
        if self.stack.len() < 1 {
            print_error(chunk, i, "flt requires one argument");
            return 0;
        }

        let value_rr = self.stack.pop().unwrap();
        let is_float;
        {
            let value_rrb = value_rr.borrow();
            match *value_rrb {
                Value::Float(_) => {
                    is_float = true;
                }
                _ => {
                    let value_opt = value_rrb.to_float();
                    match value_opt {
                        Some(n) => {
                            self.stack.push(Rc::new(RefCell::new(Value::Float(n))));
                            return 1;
                        }
                        _ => {
                            print_error(chunk, i, "unable to convert argument to float");
                            return 0;
                        }
                    }
                }
            }
        }
        if is_float {
            self.stack.push(value_rr);
        }
        return 1;
    }
}
