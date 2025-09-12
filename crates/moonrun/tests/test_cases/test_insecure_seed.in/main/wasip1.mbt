///|
/// Error codes returned by functions.
/// Not all of these error codes are returned by the functions provided by this
/// API; some are used in higher-level library layers, and others are provided
/// merely for alignment with POSIX.
/// 
/// See : https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md#-errno-variant
pub(all) suberror Errno {
  Success
  TooBig
  Acces
  Addrinuse
  Addrnotavail
  Afnosupport
  Again
  Already
  Badf
  Badmsg
  Busy
  Canceled
  Child
  Connaborted
  Connrefused
  Connreset
  Deadlock
  Destaddrreq
  Dom
  Dquot
  Exist
  Fault
  Fbig
  Hostunreach
  Idrm
  Ilseq
  Inprogress
  Intr
  Inval
  IO
  Isconn
  Isdir
  Loop
  Mfile
  Mlink
  Msgsize
  Multihop
  Nametoolong
  Netdown
  Netreset
  Netunreach
  Nfile
  Nobufs
  Nodev
  Noent
  Noexec
  Nolck
  Nolink
  Nomem
  Nomsg
  Noprotoopt
  Nospc
  Nosys
  Notconn
  Notdir
  Notempty
  Notrecoverable
  Notsock
  Notsup
  Notty
  Nxio
  Overflow
  Ownerdead
  Perm
  Pipe
  Proto
  Protonosupport
  Prototype
  Range
  Rofs
  Spipe
  Srch
  Stale
  Timedout
  Txtbsy
  Xdev
  Notcapable
} derive(Eq, Show)

///|
pub fn value(self : Errno) -> UInt {
  match self {
    Success => 0
    TooBig => 1
    Acces => 2
    Addrinuse => 3
    Addrnotavail => 4
    Afnosupport => 5
    Again => 6
    Already => 7
    Badf => 8
    Badmsg => 9
    Busy => 10
    Canceled => 11
    Child => 12
    Connaborted => 13
    Connrefused => 14
    Connreset => 15
    Deadlock => 16
    Destaddrreq => 17
    Dom => 18
    Dquot => 19
    Exist => 20
    Fault => 21
    Fbig => 22
    Hostunreach => 23
    Idrm => 24
    Ilseq => 25
    Inprogress => 26
    Intr => 27
    Inval => 28
    IO => 29
    Isconn => 30
    Isdir => 31
    Loop => 32
    Mfile => 33
    Mlink => 34
    Msgsize => 35
    Multihop => 36
    Nametoolong => 37
    Netdown => 38
    Netreset => 39
    Netunreach => 40
    Nfile => 41
    Nobufs => 42
    Nodev => 43
    Noent => 44
    Noexec => 45
    Nolck => 46
    Nolink => 47
    Nomem => 48
    Nomsg => 49
    Noprotoopt => 50
    Nospc => 51
    Nosys => 52
    Notconn => 53
    Notdir => 54
    Notempty => 55
    Notrecoverable => 56
    Notsock => 57
    Notsup => 58
    Notty => 59
    Nxio => 60
    Overflow => 61
    Ownerdead => 62
    Perm => 63
    Pipe => 64
    Proto => 65
    Protonosupport => 66
    Prototype => 67
    Range => 68
    Rofs => 69
    Spipe => 70
    Srch => 71
    Stale => 72
    Timedout => 73
    Txtbsy => 74
    Xdev => 75
    Notcapable => 76
  }
}

///|
pub fn Errno::from_value(value : UInt) -> Errno? {
  match value {
    0 => Some(Success)
    1 => Some(TooBig)
    2 => Some(Acces)
    3 => Some(Addrinuse)
    4 => Some(Addrnotavail)
    5 => Some(Afnosupport)
    6 => Some(Again)
    7 => Some(Already)
    8 => Some(Badf)
    9 => Some(Badmsg)
    10 => Some(Busy)
    11 => Some(Canceled)
    12 => Some(Child)
    13 => Some(Connaborted)
    14 => Some(Connrefused)
    15 => Some(Connreset)
    16 => Some(Deadlock)
    17 => Some(Destaddrreq)
    18 => Some(Dom)
    19 => Some(Dquot)
    20 => Some(Exist)
    21 => Some(Fault)
    22 => Some(Fbig)
    23 => Some(Hostunreach)
    24 => Some(Idrm)
    25 => Some(Ilseq)
    26 => Some(Inprogress)
    27 => Some(Intr)
    28 => Some(Inval)
    29 => Some(IO)
    30 => Some(Isconn)
    31 => Some(Isdir)
    32 => Some(Loop)
    33 => Some(Mfile)
    34 => Some(Mlink)
    35 => Some(Msgsize)
    36 => Some(Multihop)
    37 => Some(Nametoolong)
    38 => Some(Netdown)
    39 => Some(Netreset)
    40 => Some(Netunreach)
    41 => Some(Nfile)
    42 => Some(Nobufs)
    43 => Some(Nodev)
    44 => Some(Noent)
    45 => Some(Noexec)
    46 => Some(Nolck)
    47 => Some(Nolink)
    48 => Some(Nomem)
    49 => Some(Nomsg)
    50 => Some(Noprotoopt)
    51 => Some(Nospc)
    52 => Some(Nosys)
    53 => Some(Notconn)
    54 => Some(Notdir)
    55 => Some(Notempty)
    56 => Some(Notrecoverable)
    57 => Some(Notsock)
    58 => Some(Notsup)
    59 => Some(Notty)
    60 => Some(Nxio)
    61 => Some(Overflow)
    62 => Some(Ownerdead)
    63 => Some(Perm)
    64 => Some(Pipe)
    65 => Some(Proto)
    66 => Some(Protonosupport)
    67 => Some(Prototype)
    68 => Some(Range)
    69 => Some(Rofs)
    70 => Some(Spipe)
    71 => Some(Srch)
    72 => Some(Stale)
    73 => Some(Timedout)
    74 => Some(Txtbsy)
    75 => Some(Xdev)
    76 => Some(Notcapable)
    _ => None
  }
}

///|
#cfg(target="wasm")
pub fn random_get(buf : FixedArray[Byte]) -> Unit raise Errno {
  let buf_len = buf.length()
  let buf_ptr = byte_array2ptr(buf)
  let ret = random_get_ffi(buf_ptr, buf_len)
  free(buf)
  let ret = Errno::from_value(ret).unwrap()
  if ret != Success {
    raise ret
  }
}

///|
#cfg(target="wasm-gc")
pub fn random_get(buf : FixedArray[Byte]) -> Unit raise Errno {
  let buf_len = buf.length()
  let ret = random_get_ffi(0, buf_len)
  for i = 0; i < buf_len; i = i + 1 {
    buf[i] = load8_u(i)
  }
  let ret = Errno::from_value(ret).unwrap()
  if ret != Success {
    raise ret
  }
}

///|
fn random_get_ffi(buf : Int, buf_len : Int) -> UInt = "wasi_snapshot_preview1" "random_get"
