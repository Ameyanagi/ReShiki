// Isolated standard-InChI helper. The application communicates over bounded
// binary frames and never links this native code into its address space.
// Kernel: official InChI 1.07.3, MIT, see licenses/inchi/LICENSE.
#include <inchi_api.h>
#include <bcf_s.h>
#include <algorithm>
#include <array>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

namespace {
constexpr size_t max_frame=8*1024*1024;
constexpr size_t max_string=2*1024*1024;
constexpr std::array<unsigned char,8> magic={'R','S','H','I','N','C','H','I'};
static_assert(CHAR_BIT==8 && sizeof(AT_NUM)==2 && sizeof(S_CHAR)==1 && sizeof(double)==8);
static_assert(std::numeric_limits<double>::is_iec559);

struct Reader {
  const std::vector<unsigned char> &bytes;
  size_t position=0;
  unsigned char byte() {
    if(position>=bytes.size()) throw std::runtime_error("Truncated request");
    return bytes.at(position++);
  }
  uint16_t u16() { auto a=byte(); auto b=byte(); return uint16_t(a)|(uint16_t(b)<<8); }
  int16_t i16() { const auto n=u16(); return n<=32767 ? int16_t(n) : int16_t(int32_t(n)-65536); }
  int8_t i8() { const auto n=byte(); return n<=127 ? int8_t(n) : int8_t(int(n)-256); }
  uint32_t u32() { uint32_t n=0; for(unsigned i=0;i<4;++i) n|=uint32_t(byte())<<(8*i); return n; }
  double number() {
    uint64_t bits=0;
    for(unsigned i=0;i<8;++i) bits|=uint64_t(byte())<<(8*i);
    double value=0; std::memcpy(&value,&bits,sizeof(value));
    if(!std::isfinite(value)) throw std::runtime_error("Nonfinite atom coordinate");
    return value;
  }
};
struct Writer {
  std::vector<unsigned char> bytes;
  void byte(unsigned char value) {
    if(bytes.size()>=max_frame) throw std::runtime_error("Response byte limit exceeded");
    bytes.push_back(value);
  }
  void u16(uint16_t value) { byte(value&255); byte(value>>8); }
  void u32(uint32_t value) { for(unsigned i=0;i<4;++i) byte((value>>(8*i))&255); }
  void text(const char *value) {
    size_t length=0;
    if(value) { while(length<=max_string && value[length]) ++length; }
    if(length>max_string) throw std::runtime_error("Response string limit exceeded");
    u32(uint32_t(length));
    for(size_t i=0;i<length;++i) byte(static_cast<unsigned char>(value[i]));
  }
};
bool index(int id, size_t count) { return id>=0 && size_t(id)<count; }
bool direction(int code) { return code==-6 || code==-4 || code==-1 || code==0 || code==1 || code==3 || code==4 || code==6; }

Writer response() {
  std::vector<unsigned char> header(16);
  if(!std::cin.read(reinterpret_cast<char *>(header.data()),16)) throw std::runtime_error("Truncated request header");
  Reader h{header};
  for(auto byte:magic) if(h.byte()!=byte) throw std::runtime_error("Invalid request magic");
  if(h.u16()!=1 || h.byte()!=1 || h.byte()>1) throw std::runtime_error("Invalid request protocol or operation");
  const size_t size=h.u32();
  if(size>max_frame-header.size() || size<4) throw std::runtime_error("Invalid request frame length");
  std::vector<unsigned char> body(size);
  if(!std::cin.read(reinterpret_cast<char *>(body.data()),std::streamsize(size))) throw std::runtime_error("Truncated request body");
  if(std::cin.get()!=std::char_traits<char>::eof()) throw std::runtime_error("Trailing request data");
  Reader reader{body};
  const size_t atom_count=reader.u16(), stereo_count=reader.u16();
  if(atom_count>32767 || stereo_count>32767 || atom_count*39+stereo_count*12+4>body.size()) throw std::runtime_error("Invalid native counts");
  std::vector<inchi_Atom> atoms(atom_count);
  std::vector<inchi_Stereo0D> stereo(stereo_count);
  for(size_t id=0;id<atom_count;++id) {
    auto &atom=atoms.at(id);
    atom.x=reader.number(); atom.y=reader.number(); atom.z=reader.number();
    bool ended=false; size_t length=0;
    for(size_t i=0;i<6;++i) {
      const auto c=reader.byte();
      if(c==0) ended=true;
      else if(ended || !((c>='A'&&c<='Z')||(c>='a'&&c<='z')||c=='*')) throw std::runtime_error("Invalid element name");
      else ++length;
      atom.elname[i]=char(c);
    }
    if(!ended || length==0) throw std::runtime_error("Invalid element name length");
    atom.isotopic_mass=reader.i16(); atom.charge=reader.i8();
    for(auto &hydrogen:atom.num_iso_H) hydrogen=reader.i8();
    atom.radical=reader.i8();
    atom.num_bonds=reader.byte();
    if(atom.radical!=0 || atom.num_bonds>MAXVAL) throw std::runtime_error("Invalid radical or neighbor count");
    for(int b=0;b<atom.num_bonds;++b) {
      atom.neighbor[b]=reader.i16(); atom.bond_type[b]=reader.i8(); atom.bond_stereo[b]=reader.i8();
      if(!index(atom.neighbor[b],atom_count) || size_t(atom.neighbor[b])<=id || atom.bond_type[b]<0 || atom.bond_type[b]>3 || !direction(atom.bond_stereo[b])) throw std::runtime_error("Invalid stored bond");
      for(int previous=0;previous<b;++previous) if(atom.neighbor[previous]==atom.neighbor[b]) throw std::runtime_error("Duplicate stored bond");
    }
  }
  for(auto &s:stereo) {
    s.central_atom=reader.i16();
    for(auto &neighbor:s.neighbor) { neighbor=reader.i16(); if(!index(neighbor,atom_count)) throw std::runtime_error("Invalid stereo neighbor"); }
    s.type=reader.i8(); s.parity=reader.i8();
    if((s.type!=1&&s.type!=2) || s.parity<1 || s.parity>3 || (s.type==1 ? s.central_atom!=NO_ATOM : !index(s.central_atom,atom_count))) throw std::runtime_error("Invalid stereo record");
  }
  if(reader.position!=body.size()) throw std::runtime_error("Trailing frame data");
  inchi_Input input{};
  input.atom=atoms.data(); input.stereo0D=stereo.data();
  input.num_atoms=AT_NUM(atom_count); input.num_stereo0D=AT_NUM(stereo_count);
  input.szOptions=nullptr;
  struct NativeOutput {
    inchi_Output value{};
    ~NativeOutput() { FreeINCHI(&value); }
  } output;
  const int status=GetINCHI(&input,&output.value);
  Writer writer;
  writer.u16(0); writer.u16(uint16_t(status));
  writer.text(output.value.szInChI); writer.text(output.value.szMessage);
  writer.text(output.value.szLog); writer.text(output.value.szAuxInfo);
  return writer;
}
}

int main() {
#ifdef _WIN32
  if(_setmode(_fileno(stdin),_O_BINARY)==-1 || _setmode(_fileno(stdout),_O_BINARY)==-1) return 3;
#endif
  try {
    Writer header;
    for(auto c:magic) header.byte(c);
    header.u16(1); header.text(CURRENT_VER);
    std::cout.write(reinterpret_cast<const char *>(header.bytes.data()),std::streamsize(header.bytes.size()));
    std::cout.flush();
    Writer result;
    try { result=response(); }
    catch(const std::exception &error) { result.u16(1); result.text(error.what()); }
    if(header.bytes.size()+result.bytes.size()>max_frame) return 3;
    std::cout.write(reinterpret_cast<const char *>(result.bytes.data()),std::streamsize(result.bytes.size()));
    std::cout.flush();
    return std::cout ? 0 : 3;
  } catch(const std::exception &) { return 3; }
}
