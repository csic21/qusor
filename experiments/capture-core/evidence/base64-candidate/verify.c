#include <assert.h>
#include "b64digit.c"
static int reference(char c) {
    const char alphabet[]="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for (int i=0;i<64;i++) if (alphabet[i]==c) return i;
    return -1;
}
int main(void) {for(unsigned int i=0;i<256;i++) assert(b64digit((char)i)==reference((char)i));return 0;}
