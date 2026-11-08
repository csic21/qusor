static int b64digit(char c)
{
    unsigned char u = (unsigned char)c;

    if (u >= (unsigned char)'A' && u <= (unsigned char)'Z')
        return (int)(u - (unsigned char)'A');

    if (u >= (unsigned char)'a' && u <= (unsigned char)'z')
        return (int)(u - (unsigned char)'a') + 26;

    if (u >= (unsigned char)'0' && u <= (unsigned char)'9')
        return (int)(u - (unsigned char)'0') + 52;

    if (u == (unsigned char)'+')
        return 62;

    if (u == (unsigned char)'/')
        return 63;

    return -1;
}
