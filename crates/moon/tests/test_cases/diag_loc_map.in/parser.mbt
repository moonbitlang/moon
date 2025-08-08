///|
pub typealias Int as Position

///|
pub(all) enum Token {
  NUMBER(Int)
  PLUS
  MINUS
  STAR
  LPAREN
  RPAREN
} derive(Show)

///|
pub fn Token::kind(self : Token) -> TokenKind {
  match self {
    NUMBER(_) => TK_NUMBER
    PLUS => TK_PLUS
    MINUS => TK_MINUS
    STAR => TK_STAR
    LPAREN => TK_LPAREN
    RPAREN => TK_RPAREN
  }
}

///|
pub(all) enum TokenKind {
  TK_NUMBER
  TK_PLUS
  TK_MINUS
  TK_STAR
  TK_LPAREN
  TK_RPAREN
}

///|
pub impl Show for TokenKind with output(self, logger) {
  logger.write_string(
    match self {
      TK_NUMBER => "NUMBER"
      TK_PLUS => "\"+\""
      TK_MINUS => "\"-\""
      TK_STAR => "\"*\""
      TK_LPAREN => "\"(\""
      TK_RPAREN => "\")\""
    },
  )
}

///|
pub suberror ParseError {
  UnexpectedToken(Token, (Position, Position), Array[TokenKind])
  UnexpectedEndOfInput(Position, Array[TokenKind])
} derive(Show)

///|
typealias Error as YYObj

///|
priv suberror YYObj_Void

///|
priv suberror YYObj_Int Int

///|
typealias (YYSymbol) -> YYDecision as YYState

///|
typealias (Position, ArrayView[(YYObj, Position, Position)]) -> YYObj as YYAction

///|
priv enum YYDecision {
  Accept
  Shift(YYState)
  Reduce(Int, YYSymbol, YYAction)
  ReduceNoLookahead(Int, YYSymbol, YYAction)
  Error
}

///|
priv enum YYSymbol {
  T_NUMBER
  T_PLUS
  T_MINUS
  T_STAR
  T_LPAREN
  T_RPAREN
  NT_start
  NT_add
  NT_factor
  NT_term
  EOI
}

// Workaround for EOI unused warning

///|
fn init {
  match EOI {
    EOI => ()
    _ => ()
  }
}

///|
fn yy_action_0(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[0].0 is YYObj_Int(_dollar1)
  YYObj_Int(
    {
      ()
      _dollar1
    },
  )
}

///|
fn yy_action_1(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[0].0 is YYObj_Int(lhs)
  guard _args[2].0 is YYObj_Int(rhs)
  YYObj_Int(
    {
      ()
      lhs + "x" + rhs
    },
  )
}

///|
fn yy_action_2(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[0].0 is YYObj_Int(lhs)
  guard _args[2].0 is YYObj_Int(rhs)
  YYObj_Int(
    {
      ()
      lhs - rhs
    },
  )
}

///|
fn yy_action_3(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[0].0 is YYObj_Int(_dollar1)
  YYObj_Int(
    {
      ()
      _dollar1
    },
  )
}

///|
fn yy_action_4(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[0].0 is YYObj_Int(lhs)
  guard _args[2].0 is YYObj_Int(rhs)
  YYObj_Int(
    {
      ()
      lhs * rhs
    },
  )
}

///|
fn yy_action_5(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[0].0 is YYObj_Int(_dollar1)
  YYObj_Int(
    {
      ()
      _dollar1
    },
  )
}

///|
fn yy_action_6(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[0].0 is YYObj_Int(_dollar1)
  YYObj_Int(
    {
      ()
      _dollar1
    },
  )
}

///|
fn yy_action_7(
  _last_pos : Position,
  _args : ArrayView[(YYObj, Position, Position)],
) -> YYObj {
  guard _args[1].0 is YYObj_Int(_dollar2)
  YYObj_Int(
    {
      ()
      _dollar2
    },
  )
}

///|
fn yy_input(
  token : Token,
  _start_pos : Position,
  _end_pos : Position,
) -> (YYSymbol, YYObj) {
  match token {
    NUMBER(data) => (T_NUMBER, YYObj_Int(data))
    PLUS => (T_PLUS, YYObj_Void)
    MINUS => (T_MINUS, YYObj_Void)
    STAR => (T_STAR, YYObj_Void)
    LPAREN => (T_LPAREN, YYObj_Void)
    RPAREN => (T_RPAREN, YYObj_Void)
  }
}

// [0, start → • add, $]
// [1, add → • add PLUS factor, $ / PLUS / MINUS]
// [2, add → • add MINUS factor, $ / PLUS / MINUS]
// [3, add → • factor, $ / PLUS / MINUS]
// [4, factor → • factor STAR term, $ / PLUS / MINUS / STAR]
// [5, factor → • term, $ / PLUS / MINUS / STAR]
// [6, term → • NUMBER, $ / PLUS / MINUS / STAR]
// [7, term → • LPAREN add RPAREN, $ / PLUS / MINUS / STAR]
// [8, start_prime → • start, $]

///|
fn yy_state_0(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    NT_start => Shift(yy_state_1)
    T_LPAREN => Shift(yy_state_2)
    T_NUMBER => Shift(yy_state_3)
    NT_term => Shift(yy_state_4)
    NT_factor => Shift(yy_state_5)
    NT_add => Shift(yy_state_14)
    _ => Error
  }
}

// [8, start_prime → start •, $]

///|
fn yy_state_1(_lookahead : YYSymbol) -> YYDecision {
  Accept
}

// [1, add → • add PLUS factor, PLUS / MINUS / RPAREN]
// [2, add → • add MINUS factor, PLUS / MINUS / RPAREN]
// [3, add → • factor, PLUS / MINUS / RPAREN]
// [4, factor → • factor STAR term, PLUS / MINUS / STAR / RPAREN]
// [5, factor → • term, PLUS / MINUS / STAR / RPAREN]
// [6, term → • NUMBER, PLUS / MINUS / STAR / RPAREN]
// [7, term → • LPAREN add RPAREN, PLUS / MINUS / STAR / RPAREN]
// [7, term → LPAREN • add RPAREN, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_2(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_LPAREN => Shift(yy_state_2)
    T_NUMBER => Shift(yy_state_3)
    NT_term => Shift(yy_state_4)
    NT_factor => Shift(yy_state_5)
    NT_add => Shift(yy_state_8)
    _ => Error
  }
}

// [6, term → NUMBER •, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_3(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(1, NT_term, yy_action_6)
}

// [5, factor → term •, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_4(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(1, NT_factor, yy_action_5)
}

// [3, add → factor •, $ / PLUS / MINUS / RPAREN]
// [4, factor → factor • STAR term, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_5(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_STAR => Shift(yy_state_6)
    T_PLUS | T_MINUS | T_RPAREN | EOI => Reduce(1, NT_add, yy_action_3)
    _ => Error
  }
}

// [4, factor → factor STAR • term, $ / PLUS / MINUS / STAR / RPAREN]
// [6, term → • NUMBER, $ / PLUS / MINUS / STAR / RPAREN]
// [7, term → • LPAREN add RPAREN, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_6(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_LPAREN => Shift(yy_state_2)
    T_NUMBER => Shift(yy_state_3)
    NT_term => Shift(yy_state_7)
    _ => Error
  }
}

// [4, factor → factor STAR term •, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_7(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(3, NT_factor, yy_action_4)
}

// [1, add → add • PLUS factor, PLUS / MINUS / RPAREN]
// [2, add → add • MINUS factor, PLUS / MINUS / RPAREN]
// [7, term → LPAREN add • RPAREN, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_8(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_RPAREN => Shift(yy_state_9)
    T_MINUS => Shift(yy_state_10)
    T_PLUS => Shift(yy_state_12)
    _ => Error
  }
}

// [7, term → LPAREN add RPAREN •, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_9(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(3, NT_term, yy_action_7)
}

// [2, add → add MINUS • factor, $ / PLUS / MINUS / RPAREN]
// [4, factor → • factor STAR term, $ / PLUS / MINUS / STAR / RPAREN]
// [5, factor → • term, $ / PLUS / MINUS / STAR / RPAREN]
// [6, term → • NUMBER, $ / PLUS / MINUS / STAR / RPAREN]
// [7, term → • LPAREN add RPAREN, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_10(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_LPAREN => Shift(yy_state_2)
    T_NUMBER => Shift(yy_state_3)
    NT_term => Shift(yy_state_4)
    NT_factor => Shift(yy_state_11)
    _ => Error
  }
}

// [2, add → add MINUS factor •, $ / PLUS / MINUS / RPAREN]
// [4, factor → factor • STAR term, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_11(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_STAR => Shift(yy_state_6)
    T_PLUS | T_MINUS | T_RPAREN | EOI => Reduce(3, NT_add, yy_action_2)
    _ => Error
  }
}

// [1, add → add PLUS • factor, $ / PLUS / MINUS / RPAREN]
// [4, factor → • factor STAR term, $ / PLUS / MINUS / STAR / RPAREN]
// [5, factor → • term, $ / PLUS / MINUS / STAR / RPAREN]
// [6, term → • NUMBER, $ / PLUS / MINUS / STAR / RPAREN]
// [7, term → • LPAREN add RPAREN, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_12(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_LPAREN => Shift(yy_state_2)
    T_NUMBER => Shift(yy_state_3)
    NT_term => Shift(yy_state_4)
    NT_factor => Shift(yy_state_13)
    _ => Error
  }
}

// [1, add → add PLUS factor •, $ / PLUS / MINUS / RPAREN]
// [4, factor → factor • STAR term, $ / PLUS / MINUS / STAR / RPAREN]

///|
fn yy_state_13(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_STAR => Shift(yy_state_6)
    T_PLUS | T_MINUS | T_RPAREN | EOI => Reduce(3, NT_add, yy_action_1)
    _ => Error
  }
}

// [0, start → add •, $]
// [1, add → add • PLUS factor, $ / PLUS / MINUS]
// [2, add → add • MINUS factor, $ / PLUS / MINUS]

///|
fn yy_state_14(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_MINUS => Shift(yy_state_10)
    T_PLUS => Shift(yy_state_12)
    EOI => Reduce(1, NT_start, yy_action_0)
    _ => Error
  }
}

///|
fn[T] yy_parse(
  tokens : Array[(Token, Position, Position)],
  start : YYState,
  return_ : (YYObj) -> T,
) -> T raise ParseError {
  let mut cursor = 0
  let mut state_stack = @list.singleton(start)
  let data_stack : Array[(YYObj, Position, Position)] = []
  let mut last_pos = tokens[0].1
  let mut state = start
  let mut lookahead : (YYSymbol, (YYObj, Position, Position), Token?)? = None
  let mut last_shifted_state_stack = state_stack
  while true {
    let decision = match state(EOI) {
      ReduceNoLookahead(_) | Accept as t => t
      _ =>
        match lookahead {
          Some(la) => state(la.0)
          None =>
            if cursor < tokens.length() {
              let (token, start_pos, end_pos) = tokens[cursor]
              cursor += 1
              let (symbol, data) = yy_input(token, start_pos, end_pos)
              lookahead = Some(
                (symbol, (data, start_pos, end_pos), Some(token)),
              )
              state(symbol)
            } else {
              lookahead = Some((EOI, (YYObj_Void, last_pos, last_pos), None))
              state(EOI)
            }
        }
    }
    match decision {
      Accept => return return_(data_stack.unsafe_pop().0)
      Shift(next_state) => {
        guard lookahead is Some(la)
        data_stack.push(la.1)
        state_stack = state_stack.add(next_state)
        last_shifted_state_stack = state_stack
        state = next_state
        last_pos = la.1.2
        lookahead = None
      }
      Reduce(count, symbol, action)
      | ReduceNoLookahead(count, symbol, action) =>
        loop (count, symbol, action) {
          _ => {
            let args = data_stack[data_stack.length() - count:]
            let data = action(last_pos, args)
            let (start_pos, end_pos) = if args.length() == 0 {
              (last_pos, last_pos)
            } else {
              (args[0].1, args[args.length() - 1].2)
            }
            for i in 0..<count {
              ignore(data_stack.unsafe_pop())
              state_stack = state_stack.unsafe_tail()
            }
            state = state_stack.unsafe_head()
            data_stack.push((data, start_pos, end_pos))
            match state(symbol) {
              Accept => return return_(data_stack.unsafe_pop().0)
              Shift(next_state) => {
                state_stack = state_stack.add(next_state)
                state = next_state
              }
              Reduce(count, symbol, action)
              | ReduceNoLookahead(count, symbol, action) =>
                continue (count, symbol, action)
              _ => panic()
            }
          }
        }
      Error => {
        let (_, (_, start_pos, end_pos), token) = lookahead.unwrap()
        error(last_shifted_state_stack, token, (start_pos, end_pos))
      }
    }
  }
  panic()
}

///|
fn error(
  stack : @list.List[YYState],
  token : Token?,
  loc : (Position, Position),
) -> Unit raise ParseError {
  let expected = []
  fn try_add(symbol : YYSymbol, kind : TokenKind) {
    fn go(stack : @list.List[YYState]) {
      match stack {
        Empty => ()
        More(state, ..) =>
          match state(symbol) {
            Accept | Shift(_) => expected.push(kind)
            Reduce(count, symbol, _) | ReduceNoLookahead(count, symbol, _) => {
              fn inner_go(stack : @list.List[YYState], count, symbol) {
                let stack = stack.drop(count)
                guard stack is More(state, ..)
                match state(symbol) {
                  Shift(state) => go(stack.add(state))
                  Reduce(count, symbol, _)
                  | ReduceNoLookahead(count, symbol, _) =>
                    inner_go(stack, count, symbol)
                  _ => panic()
                }
              }

              inner_go(stack, count, symbol)
            }
            Error => ()
          }
      }
    }

    go(stack)
  }

  for
    term in [
      (T_NUMBER, TK_NUMBER),
      (T_PLUS, TK_PLUS),
      (T_MINUS, TK_MINUS),
      (T_STAR, TK_STAR),
      (T_LPAREN, TK_LPAREN),
      (T_RPAREN, TK_RPAREN),
    ] {
    try_add(term.0, term.1)
  }
  match token {
    None => raise UnexpectedEndOfInput(loc.1, expected)
    Some(token) => raise UnexpectedToken(token, loc, expected)
  }
}

///|
pub fn start(
  tokens : Array[(Token, Position, Position)],
) -> Int raise ParseError {
  yy_parse(tokens, yy_state_0, fn {
    YYObj_Int(result) => result
    _ => panic()
  })
}

///|
fn footer() -> Unit {
  ()
}

///|
fn init {
  ignore(footer)
}
