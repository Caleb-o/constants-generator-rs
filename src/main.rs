use std::{
    fs::{self, File},
    io::{Read, Write},
    rc::Rc,
};

type ObjectPool = Vec<Rc<Object>>;

fn main() {
    let args = std::env::args();
    if args.len() != 2 {
        println!("Malformed input");
        return;
    }

    let file_name = "constants";

    let mut objs = Vec::<Rc<Object>>::new();
    let mut values = Vec::<Value>::new();

    match args.skip(1).next().unwrap().as_str() {
        "l" => {
            let mut f = fs::File::open(file_name).expect("Could not open file");
            load_values_from_disk(&mut f, &mut values, &mut objs);

            for value in values {
                value.display();
            }
        }
        "s" => {
            values.extend([
                Value::Int(100),
                Value::Bool(false),
                Value::from_string("Hello!", &mut objs),
                Value::from_function_literal(
                    "foo_bar",
                    1,
                    &[ByteCode::ConstantByte as u8, 3, ByteCode::Return as u8],
                    &mut objs,
                ),
            ]);

            let mut f = fs::File::create(file_name).expect("Could not open file");
            write_values_to_disk(&mut f, &values);
        }
        s => panic!("Invalid '{s}'"),
    }
}

fn write_values_to_disk(file: &mut File, values: &[Value]) {
    file.write(&values.len().to_be_bytes()).unwrap();

    for value in values {
        value.write(file);
    }

    println!("{} constants written to file", values.len());
}

fn load_values_from_disk(file: &mut File, values: &mut Vec<Value>, pool: &mut ObjectPool) {
    let constants_to_read = read_usize(file);
    values.reserve(constants_to_read);

    for _ in 0..constants_to_read {
        let byte_id = read_u8(file);
        values.push(Value::read(file, byte_id, pool));
    }

    println!("{} constants read from file", values.len());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ByteCode {
    ConstantByte,
    Return,
}

#[derive(Debug, Clone)]
enum Value {
    Int(i32),
    Bool(bool),
    Object(Rc<Object>),
}

#[derive(Debug, Clone)]
enum Object {
    String(String),
    Function {
        identifier: String,
        param_count: u8,
        code: Vec<u8>,
    },
}

trait ConstantIO {
    fn to_type_id(&self) -> u8;
    fn read(file: &mut File, byte_id: u8, pool: &mut ObjectPool) -> Value;
    fn write(&self, file: &mut File);
}

impl Value {
    fn from_string(str: &'static str, pool: &mut ObjectPool) -> Value {
        let v = Rc::new(Object::String(str.to_string()));
        pool.push(Rc::clone(&v));
        Value::Object(v)
    }

    fn from_function_literal(
        id: &'static str,
        param_count: u8,
        code: &[u8],
        pool: &mut ObjectPool,
    ) -> Value {
        let v = Rc::new(Object::Function {
            identifier: id.to_string(),
            param_count,
            code: code.iter().map(|b| *b).collect(),
        });
        pool.push(Rc::clone(&v));
        Value::Object(v)
    }

    fn display(&self) {
        match self {
            Value::Int(i) => println!("{i}"),
            Value::Bool(b) => println!("{b}"),
            Value::Object(o) => o.display(),
        }
    }
}

impl Object {
    fn display(&self) {
        match &*self {
            Object::String(s) => println!("{s}"),
            Object::Function {
                identifier,
                param_count,
                code,
            } => {
                println!("func<'{identifier}', {param_count}>");

                let mut ip = 0;
                while ip < code.len() {
                    let op = unsafe { std::mem::transmute::<u8, ByteCode>(code[ip]) };
                    print!("{ip:04} ");
                    match op {
                        ByteCode::ConstantByte => byte_instruction(&mut ip, "CONSTANT_BYTE", &code),
                        ByteCode::Return => simple_instruction(&mut ip, "RETURN"),
                    }
                }
            }
        }
    }
}

impl ConstantIO for Value {
    fn to_type_id(&self) -> u8 {
        match &*self {
            Value::Int(_) => 0,
            Value::Bool(_) => 1,
            Value::Object(o) => o.to_type_id(),
        }
    }

    fn read(file: &mut File, byte_id: u8, pool: &mut ObjectPool) -> Self {
        match byte_id {
            0 => Value::Int(read_i32(file)),
            1 => Value::Bool(read_u8(file) == 1),
            _ => Object::read(file, byte_id, pool),
        }
    }

    fn write(&self, file: &mut File) {
        let byte_id = self.to_type_id();
        file.write(&[byte_id]).unwrap();

        match self {
            Value::Int(i) => _ = file.write(&i.to_be_bytes()).unwrap(),
            Value::Bool(b) => _ = file.write(&[if *b { 1 } else { 0 }]).unwrap(),
            Value::Object(o) => o.write(file),
        }
    }
}

impl ConstantIO for Object {
    fn to_type_id(&self) -> u8 {
        match &*self {
            Object::String(_) => 2,
            Object::Function { .. } => 3,
        }
    }

    fn read(file: &mut File, byte_id: u8, pool: &mut ObjectPool) -> Value {
        match byte_id {
            // String
            2 => {
                let str = read_string(file);
                let obj = Rc::new(Object::String(str));
                pool.push(Rc::clone(&obj));
                Value::Object(obj)
            }
            3 => {
                let identifier = read_string(file);
                let param_count = read_u8(file);
                let code = read_bytes(file);

                let obj = Rc::new(Object::Function {
                    identifier,
                    param_count,
                    code,
                });
                pool.push(Rc::clone(&obj));
                Value::Object(obj)
            }
            _ => unreachable!("Invalid ID"),
        }
    }

    fn write(&self, file: &mut File) {
        match &*self {
            Object::String(s) => write_string(file, s),
            Object::Function {
                identifier,
                param_count,
                code,
            } => {
                write_string(file, identifier);
                file.write(&param_count.to_be_bytes()).unwrap();

                file.write(&code.len().to_be_bytes()).unwrap();
                file.write(&code).unwrap();
            }
        }
    }
}

fn write_string(file: &mut File, str: &String) {
    file.write(&str.len().to_be_bytes()).unwrap();
    file.write(str.as_bytes()).unwrap();
}

fn read_bytes(file: &mut File) -> Vec<u8> {
    let size = read_usize(file);
    let mut buffer = (0..size).map(|_| 0).collect::<Vec<u8>>();
    file.read_exact(&mut buffer).unwrap();

    buffer
}

fn read_string(file: &mut File) -> String {
    let buffer = read_bytes(file);
    String::from_utf8(buffer).unwrap()
}

fn read_u8(file: &mut File) -> u8 {
    let mut buffer = [0u8; 1];
    file.read_exact(&mut buffer).unwrap();

    u8::from_be_bytes(buffer)
}

fn read_i32(file: &mut File) -> i32 {
    let mut buffer = [0u8; 4];
    file.read_exact(&mut buffer).unwrap();

    i32::from_be_bytes(buffer)
}

fn read_usize(file: &mut File) -> usize {
    let mut buffer = [0u8; 8];
    file.read_exact(&mut buffer).unwrap();

    usize::from_be_bytes(buffer)
}

fn simple_instruction(ip: &mut usize, label: &'static str) {
    println!("{label}");
    *ip += 1;
}

fn byte_instruction(ip: &mut usize, label: &'static str, code: &[u8]) {
    println!("{label} {}", code[*ip + 1]);
    *ip += 2;
}
