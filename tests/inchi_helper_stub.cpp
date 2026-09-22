// Development-only failure injection. No native kernel is linked here.
#include <chrono>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <thread>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#include <process.h>
#define get_pid _getpid
#else
#include <unistd.h>
#define get_pid getpid
#endif
void u16(uint16_t n) { std::cout.put(char(n&255)); std::cout.put(char(n>>8)); }
void u64(uint64_t n) { for(unsigned i=0;i<8;++i) std::cout.put(char((n>>(8*i))&255)); }
void u32(uint32_t n) { for(unsigned i=0;i<4;++i) std::cout.put(char((n>>(8*i))&255)); }
void text(const std::string &s) { u32(uint32_t(s.size())); std::cout.write(s.data(),std::streamsize(s.size())); }
int main(int argc,char **argv) {
#ifdef _WIN32
  if(_setmode(_fileno(stdin),_O_BINARY)==-1 || _setmode(_fileno(stdout),_O_BINARY)==-1) return 3;
#endif
  if(argc!=1) return 2;
  const std::filesystem::path executable=std::filesystem::absolute(argv[0]);
  const auto mode=executable.stem().string();
  std::ofstream(executable.parent_path()/"pid") << get_pid();
  if(mode=="hang" || mode=="no-read") { for(;;) std::this_thread::sleep_for(std::chrono::seconds(1)); }
  std::string request;
  while(std::cin.get()!=std::char_traits<char>::eof()) {}
  if(mode=="exit") { std::cerr << "stub exit"; return 17; }
  if(mode=="oversized" || mode=="stderr") {
    auto &output=mode=="stderr" ? std::cerr : std::cout;
    for(size_t i=0;i<9*1024*1024;++i) output.put('x');
    output.flush();
    for(;;) std::this_thread::sleep_for(std::chrono::seconds(1));
  }
  std::cout << "RSHINCHI"; u16(mode=="protocol" ? 9 : 2); text(mode=="version" ? "1.07.4" : "1.07.3");
  if(mode.starts_with("resource")) {
    u16(2); u16(mode=="resource-scope" ? 9 : 1);
    u16(mode=="resource-unavailable" ? 2 : mode=="resource-reason" ? 9 : 1);
    u64(mode=="resource-budget" ? 0 : 64*1024*1024);
    if(mode=="resource-truncated") return 0;
    u64(mode=="resource-used" ? 64*1024*1024+1 : 0); u64(1024);
    return 0;
  }
  if(mode=="truncated") { u16(0); return 0; }
  if(mode=="rejected") { u16(1); text("stub rejection"); return 0; }
  u16(0); u16(mode=="status" ? 127 : 0);
  if(mode=="string-length") { u32(0xffffffff); return 0; }
  text(mode=="nonstandard" ? "InChI=1/C" : "InChI=1S/CH4/h1H4");
  text(mode=="utf8" ? std::string(1,char(255)) : ""); text(""); text("AuxInfo=1/0/N:1");
  if(mode=="trailing") std::cout.put('x');
  return 0;
}
