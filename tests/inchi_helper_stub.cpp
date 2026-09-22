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
  if(mode.starts_with("read-")) {
    u16(3); u32(mode=="read-status" ? 99 : 1); text(""); text("");
    u64(uint64_t(1)<<50); u64(7); u64(8); u64(9);
    u16(mode=="read-counts" ? 32768 : 2); u16(1);
    if(mode=="read-counts" || mode=="read-truncated") return 0;
    for(unsigned atom=0;atom<2;++atom) {
      u64(mode=="read-coordinate" ? 0x7ff8000000000000ULL : 0); u64(0); u64(0);
      std::cout.put(mode=="read-element" ? char(255) : atom==0 ? 'C' : 'N');
      for(unsigned n=0;n<5;++n) std::cout.put(0);
      u16(10001); std::cout.put(char(-1));
      for(int h:{-1,1,2,3}) std::cout.put(char(h));
      std::cout.put(char(2)); std::cout.put(mode=="read-adjacency" ? char(21) : atom==0 ? char(2) : char(1));
      for(unsigned b=0;b<(atom==0?2:1);++b) {
        u16(mode=="read-index" ? 9 : atom==0 ? 1 : 0);
        std::cout.put(char(-1)); std::cout.put(char(-4));
      }
    }
    u16(mode=="read-stereo" ? 2 : 0); u16(0); u16(1); u16(1); u16(0);
    std::cout.put(char(3)); std::cout.put(char(4));
    if(mode=="read-trailing") std::cout.put('x');
    return 0;
  }
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
