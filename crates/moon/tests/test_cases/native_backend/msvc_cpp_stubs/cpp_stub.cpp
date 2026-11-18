consteval int answer() { return 42; }

extern "C" int cpp_answer() { return answer(); }
