consteval int answer() { return 23; }

extern "C" int cpp_answer() { return answer(); }
