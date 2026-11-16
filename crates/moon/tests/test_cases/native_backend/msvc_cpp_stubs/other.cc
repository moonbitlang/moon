consteval int other_value() { return 10; }

extern "C" int other_answer() { return other_value(); }
