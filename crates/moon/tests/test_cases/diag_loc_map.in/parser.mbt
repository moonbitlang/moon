pub(all) typealias Position = Int

pub(all) enum Token {
  NUMBER(Int)
  PLUS
  MINUS
  STAR
  LPAREN
  RPAREN
} derive(Show)

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

pub(all) enum TokenKind {
  TK_NUMBER
  TK_PLUS
  TK_MINUS
  TK_STAR
  TK_LPAREN
  TK_RPAREN
}

pub impl Show for TokenKind with output(self, logger) {
  logger.write_string(
    match self {
      TK_NUMBER => "NUMBER"
      TK_PLUS => "\"+\""
      TK_MINUS => "\"-\""
      TK_STAR => "\"*\""
      TK_LPAREN => "\"(\""
      TK_RPAREN => "\")\""
    }
  )
}

pub type! ParseError {
  UnexpectedToken(Token, (Position, Position), Array[TokenKind])
  UnexpectedEndOfInput(Position, Array[TokenKind])
} derive(Show)

typealias YYObj = Error

priv type! YYObj_Void

priv type! YYObj_Int Int

typealias YYState = (YYSymbol) -> YYDecision

typealias YYAction = (Position, ArrayView[(YYObj, Position, Position)]) -> YYObj

priv enum YYDecision {
  Accept
  Shift(YYState)
  Reduce(Int, YYSymbol, YYAction)
  ReduceNoLookahead(Int, YYSymbol, YYAction)
  Error
}

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
fn init {
  match EOI {
    EOI => ()
    _ => ()
  }
}

fn yy_action_0(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(_dollar1) = _args[0].0
  YYObj_Int({();  _dollar1 })
}

fn yy_action_1(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(lhs) = _args[0].0
  guard let YYObj_Int(rhs) = _args[2].0
  YYObj_Int({();  lhs + 'x' + rhs })
}

fn yy_action_2(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(lhs) = _args[0].0
  guard let YYObj_Int(rhs) = _args[2].0
  YYObj_Int({();  lhs - rhs })
}

fn yy_action_3(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(_dollar1) = _args[0].0
  YYObj_Int({();  _dollar1 })
}

fn yy_action_4(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(lhs) = _args[0].0
  guard let YYObj_Int(rhs) = _args[2].0
  YYObj_Int({();  lhs * rhs })
}

fn yy_action_5(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(_dollar1) = _args[0].0
  YYObj_Int({();  _dollar1 })
}

fn yy_action_6(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(_dollar1) = _args[0].0
  YYObj_Int({();  _dollar1 })
}

fn yy_action_7(_last_pos : Position, _args : ArrayView[(YYObj, Position, Position)]) -> YYObj {
  guard let YYObj_Int(_dollar2) = _args[1].0
  YYObj_Int({();  _dollar2 })
}

fn yy_input(token : Token, _start_pos : Position, _end_pos : Position) -> (YYSymbol, YYObj) {
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
fn yy_state_3(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(1, NT_term, yy_action_6)
}

// [5, factor → term •, $ / PLUS / MINUS / STAR / RPAREN]
fn yy_state_4(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(1, NT_factor, yy_action_5)
}

// [3, add → factor •, $ / PLUS / MINUS / RPAREN]
// [4, factor → factor • STAR term, $ / PLUS / MINUS / STAR / RPAREN]
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
fn yy_state_6(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_LPAREN => Shift(yy_state_2)
    T_NUMBER => Shift(yy_state_3)
    NT_term => Shift(yy_state_7)
    _ => Error
  }
}

// [4, factor → factor STAR term •, $ / PLUS / MINUS / STAR / RPAREN]
fn yy_state_7(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(3, NT_factor, yy_action_4)
}

// [1, add → add • PLUS factor, PLUS / MINUS / RPAREN]
// [2, add → add • MINUS factor, PLUS / MINUS / RPAREN]
// [7, term → LPAREN add • RPAREN, $ / PLUS / MINUS / STAR / RPAREN]
fn yy_state_8(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_RPAREN => Shift(yy_state_9)
    T_MINUS => Shift(yy_state_10)
    T_PLUS => Shift(yy_state_12)
    _ => Error
  }
}

// [7, term → LPAREN add RPAREN •, $ / PLUS / MINUS / STAR / RPAREN]
fn yy_state_9(_lookahead : YYSymbol) -> YYDecision {
  ReduceNoLookahead(3, NT_term, yy_action_7)
}

// [2, add → add MINUS • factor, $ / PLUS / MINUS / RPAREN]
// [4, factor → • factor STAR term, $ / PLUS / MINUS / STAR / RPAREN]
// [5, factor → • term, $ / PLUS / MINUS / STAR / RPAREN]
// [6, term → • NUMBER, $ / PLUS / MINUS / STAR / RPAREN]
// [7, term → • LPAREN add RPAREN, $ / PLUS / MINUS / STAR / RPAREN]
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
fn yy_state_14(_lookahead : YYSymbol) -> YYDecision {
  match _lookahead {
    T_MINUS => Shift(yy_state_10)
    T_PLUS => Shift(yy_state_12)
    EOI => Reduce(1, NT_start, yy_action_0)
    _ => Error
  }
}

fn yy_parse[T](
  tokens : Array[(Token, Position, Position)],
  start : YYState,
  return_ : (YYObj) -> T
) -> T!ParseError {
  let mut cursor = 0
  let mut state_stack : @immut/list.T[YYState] = Cons(start, Nil)
  let data_stack : Array[(YYObj, Position, Position)] = []
  let mut last_pos = tokens[0].1
  let mut state = start
  let mut lookahead : Option[(YYSymbol, (YYObj, Position, Position), Token?)] = None
  let mut last_shifted_state_stack = state_stack
  while true {
    let decision = match state(EOI) {
      ReduceNoLookahead(_) | Accept as t => t
      _ => {
        match lookahead {
          Some(la) => state(la.0)
          None => {
            if cursor < tokens.length() {
              let (token, start_pos, end_pos) = tokens[cursor]
              cursor += 1
              let (symbol, data) = yy_input(token, start_pos, end_pos)
              lookahead = Some((symbol, (data, start_pos, end_pos), Some(token)))
              state(symbol)
            } else {
              lookahead = Some((EOI, (YYObj_Void, last_pos, last_pos), None))
              state(EOI)
            }
          }
        }
      }
    }
    match decision {
      Accept => return return_(data_stack.unsafe_pop().0)
      Shift(next_state) => {
        guard let Some(la) = lookahead
        data_stack.push(la.1)
        state_stack = Cons(next_state, state_stack)
        last_shifted_state_stack = state_stack
        state = next_state
        last_pos = la.1.2
        lookahead = None
      }
      Reduce(count, symbol, action)
      | ReduceNoLookahead(count, symbol, action) => {
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
              state_stack = state_stack.tail()
            }
            state = state_stack.unsafe_head()
            data_stack.push((data, start_pos, end_pos))
            match state(symbol) {
              Accept => return return_(data_stack.unsafe_pop().0)
              Shift(next_state) => {
                state_stack = Cons(next_state, state_stack)
                state = next_state
              }
              Reduce(count, symbol, action)
              | ReduceNoLookahead(count, symbol, action) => continue (count, symbol, action)
              _ => panic()
            }
          }
        }
      }
      Error => {
        let (_, (_, start_pos, end_pos), token) = lookahead.unwrap()
        error!(last_shifted_state_stack, token, (start_pos, end_pos))
      }
    }
  }
  panic()
}

fn error(stack : @immut/list.T[YYState], token : Token?, loc : (Position, Position)) -> Unit!ParseError {
  let expected = []
  fn try_add(symbol : YYSymbol, kind : TokenKind) {
    fn go(stack : @immut/list.T[YYState]) {
      match stack {
        Nil => ()
        Cons(state, _) => {
          match state(symbol) {
            Accept | Shift(_) => expected.push(kind)
            Reduce(count, symbol, _) | ReduceNoLookahead(count, symbol, _) => {
              fn inner_go(stack : @immut/list.T[YYState], count, symbol) {
                let stack = stack.drop(count)
                guard let Cons(state, _) = stack
                match state(symbol) {
                  Shift(state) => go(Cons(state, stack))
                  Reduce(count, symbol, _) | ReduceNoLookahead(count, symbol, _) => inner_go(stack, count, symbol)
                  _ => panic()
                }
              }
              inner_go(stack, count, symbol)
            }
            Error => ()
          }
        }
      }
    }
    go(stack)
  }
  for term in [(T_NUMBER, TK_NUMBER), (T_PLUS, TK_PLUS), (T_MINUS, TK_MINUS), (T_STAR, TK_STAR), (T_LPAREN, TK_LPAREN), (T_RPAREN, TK_RPAREN)] {
    try_add(term.0, term.1)
  }
  match token {
    None => raise UnexpectedEndOfInput(loc.1, expected)
    Some(token) => raise UnexpectedToken(token, loc, expected)
  }
}

pub fn start(tokens: Array[(Token, Position, Position)]) -> Int!ParseError {
  yy_parse!(
    tokens,
    yy_state_0,
    fn {
      YYObj_Int(result) => result
      _ => panic()
    },
  )
}


fn footer() -> Unit {
  ()
}

fn init {
  ignore(footer)
}
