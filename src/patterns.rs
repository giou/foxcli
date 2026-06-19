pub fn pattern_help() -> &'static str {
    r#"Pattern passwords are the characters each dot maps to, in draw order.
Dots map to characters starting at '1': dots 1-9 are '1'-'9', and dots 10 and up
continue through ASCII (10=':', 11=';', 12='<', 13='=', 14='>', 15='?', 16='@', 17='A', ...).

3x3 (dot numbers):
  1 2 3
  4 5 6
  7 8 9
  Example: 1 -> 4 -> 7 -> 8 -> 9
    ●  ·  ·
    │
    ●  ·  ·
    │
    ●──●──●
  fox decrypt 14789

4x4 (dot numbers):
   1  2  3  4
   5  6  7  8
   9 10 11 12
  13 14 15 16
  Example: 1 -> 5 -> 9 -> 13
    ●  ·  ·  ·
    │
    ●  ·  ·  ·
    │
    ●  ·  ·  ·
    │
    ●  ·  ·  ·
  fox decrypt 159=

5x5 (dot numbers):
   1  2  3  4  5
   6  7  8  9 10
  11 12 13 14 15
  16 17 18 19 20
  21 22 23 24 25
  Example: 1 -> 6 -> 11 -> 16 -> 21
    ●  ·  ·  ·  ·
    │
    ●  ·  ·  ·  ·
    │
    ●  ·  ·  ·  ·
    │
    ●  ·  ·  ·  ·
    │
    ●  ·  ·  ·  ·
  fox decrypt 16;@E

For PINs and text passwords, pass the value directly: fox decrypt <password>"#
}
