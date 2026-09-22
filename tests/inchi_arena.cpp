// Independent allocator checks; no chemistry kernel is linked.
#include "../tools/inchi-helper/arena.h"
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <string_view>
#include <vector>

void check(bool condition) { if(!condition) std::exit(9); }
void failure(int reason,size_t budget,size_t used,size_t requested) {
    std::printf("reason=%d budget=%zu used=%zu requested=%zu\n",reason,budget,used,requested);
    std::exit(reason==1 ? 21 : 22);
}
int main(int argc,char **argv) {
    if(argc!=2) return 2;
    const std::string_view mode=argv[1];
    constexpr size_t cap=1024*1024;
    rsh_heap_initialize(mode=="tiny" ? 1 : cap,failure);
    if(mode=="overflow") rsh_kernel_calloc(std::numeric_limits<size_t>::max(),2);
    if(mode=="huge") rsh_kernel_malloc(cap);
    if(mode=="strdup") {
        std::vector<char> value(cap, 'x');
        value.back()='\0';
        rsh_kernel_strdup(value.data());
        return 9;
    }
    if(mode=="fragmentation") {
        std::vector<void *> blocks;
        for(size_t i=0;i<64;++i) blocks.push_back(rsh_kernel_malloc(8192));
        for(size_t i=0;i<64;i+=2) rsh_kernel_free(blocks[i]);
        rsh_kernel_malloc(600*1024); // Total free bytes exist, but no fitting block.
        return 9;
    }
    if(mode!="stress") return 2;
    struct Slot { unsigned char *pointer=nullptr; size_t length=0; unsigned char value=0; };
    std::vector<Slot> slots(64);
    uint64_t random=0x1635262;
    for(size_t step=0;step<20000;++step) {
        random=random*6364136223846793005ULL+1442695040888963407ULL;
        auto &slot=slots[(random>>32)%slots.size()];
        for(size_t i=0;i<slot.length;++i) check(slot.pointer[i]==slot.value);
        const size_t length=(random>>12)%2048;
        if((random&3)==0) {
            rsh_kernel_free(slot.pointer); slot={};
        } else if((random&3)==1 && !slot.pointer) {
            slot.pointer=static_cast<unsigned char *>(rsh_kernel_calloc(length,1));
            for(size_t i=0;i<length;++i) check(slot.pointer[i]==0);
            slot.length=length;
        } else {
            auto *next=static_cast<unsigned char *>(rsh_kernel_realloc(slot.pointer,length));
            if(slot.pointer && length==0) check(next==nullptr);
            else for(size_t i=0;i<std::min(length,slot.length);++i) check(next[i]==slot.value);
            slot.pointer=next; slot.length=length;
        }
        if(slot.pointer) {
            check(reinterpret_cast<std::uintptr_t>(slot.pointer)%alignof(std::max_align_t)==0);
            slot.value=static_cast<unsigned char>(random);
            std::memset(slot.pointer,slot.value,slot.length);
        }
        check(rsh_heap_used()<=cap && rsh_heap_peak()<=cap);
    }
    for(auto &slot:slots) rsh_kernel_free(slot.pointer);
    check(rsh_heap_used()==0);
    // Coalescing restores enough capacity for one almost-arena-sized block.
    void *large=rsh_kernel_malloc(cap-256);
    rsh_kernel_free(large);
    void *zero=rsh_kernel_malloc(0); check(zero!=nullptr); rsh_kernel_free(zero);
    rsh_kernel_free(nullptr);
    check(rsh_kernel_strdup(nullptr)==nullptr && rsh_heap_used()==0);
    for(const char *source : {"", "C", "native string duplication", "before\0after"}) {
        const size_t bytes=std::strlen(source)+1;
        char *copy=rsh_kernel_strdup(source);
        check(copy!=source && std::memcmp(copy,source,bytes)==0);
        check(rsh_heap_used()>=bytes && rsh_heap_used()<=cap);
        if(bytes>1) copy[0]='!';
        check(source[0]!='!');
        rsh_kernel_free(copy);
        check(rsh_heap_used()==0);
    }
    check(rsh_heap_used()==0);
    std::printf("peak=%zu capacity=%zu\n",rsh_heap_peak(),cap);
    rsh_heap_destroy();
    rsh_heap_initialize(cap,failure);
    check(rsh_heap_used()==0 && rsh_heap_peak()==0);
    rsh_heap_destroy();
    return 0;
}
