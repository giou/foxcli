pub fn pattern_help() -> &'static str {
    "Pattern passwords are entered as row-major dot numbers, concatenated in draw order.\n\
\n\
3x3:\n\
  1 2 3\n\
  4 5 6\n\
  7 8 9\n\
  Example: fox decrypt 14789\n\
\n\
4x4:\n\
  1  2  3  4\n\
  5  6  7  8\n\
  9 10 11 12\n\
 13 14 15 16\n\
  Example: fox decrypt 15913\n\
\n\
5x5:\n\
  1  2  3  4  5\n\
  6  7  8  9 10\n\
 11 12 13 14 15\n\
 16 17 18 19 20\n\
 21 22 23 24 25\n\
  Example: fox decrypt 16111621\n\
\n\
For PINs and text passwords, pass the value directly: fox decrypt <password>\n\
Legacy pre-Marshmallow non-3x3 ROMs may use OrangeFox/TWRP's old hex pattern format."
}
