use std::{
    collections::{HashMap, HashSet},
    io::Read,
};

use flate2::bufread::GzDecoder;
use ichiran::{
    charset,
    romanize::{Clause, Conjugation, Meta, Root, Segment, Term, Word},
};
use imgui::*;
use imgui_winit_support::WinitPlatform;

// https://github.com/ocornut/imgui/blob/83d22f4e480c6f71ffc1514c2453feed0fce2733/imgui_draw.cpp#L2951
const JA_ACC_OFF_0X4_E00_UTF8: &[u16] = &[
    0, 1, 2, 4, 1, 1, 1, 1, 2, 1, 3, 3, 2, 2, 1, 5, 3, 5, 7, 5, 6, 1, 2, 1, 7, 2, 6, 3, 1, 8, 1, 1,
    4, 1, 1, 18, 2, 11, 2, 6, 2, 1, 2, 1, 5, 1, 2, 1, 3, 1, 2, 1, 2, 3, 3, 1, 1, 2, 3, 1, 1, 1, 12,
    7, 9, 1, 4, 5, 1, 1, 2, 1, 10, 1, 1, 9, 2, 2, 4, 5, 6, 9, 3, 1, 1, 1, 1, 9, 3, 18, 5, 2, 2, 2,
    2, 1, 6, 3, 7, 1, 1, 1, 1, 2, 2, 4, 2, 1, 23, 2, 10, 4, 3, 5, 2, 4, 10, 2, 4, 13, 1, 6, 1, 9,
    3, 1, 1, 6, 6, 7, 6, 3, 1, 2, 11, 3, 2, 2, 3, 2, 15, 2, 2, 5, 4, 3, 6, 4, 1, 2, 5, 2, 12, 16,
    6, 13, 9, 13, 2, 1, 1, 7, 16, 4, 7, 1, 19, 1, 5, 1, 2, 2, 7, 7, 8, 2, 6, 5, 4, 9, 18, 7, 4, 5,
    9, 13, 11, 8, 15, 2, 1, 1, 1, 2, 1, 2, 2, 1, 2, 2, 8, 2, 9, 3, 3, 1, 1, 4, 4, 1, 1, 1, 4, 9, 1,
    4, 3, 5, 5, 2, 7, 5, 3, 4, 8, 2, 1, 13, 2, 3, 3, 1, 14, 1, 1, 4, 5, 1, 3, 6, 1, 5, 2, 1, 1, 3,
    3, 3, 3, 1, 1, 2, 7, 6, 6, 7, 1, 4, 7, 6, 1, 1, 1, 1, 1, 12, 3, 3, 9, 5, 2, 6, 1, 5, 6, 1, 2,
    3, 18, 2, 4, 14, 4, 1, 3, 6, 1, 1, 6, 3, 5, 5, 3, 2, 2, 2, 2, 12, 3, 1, 4, 2, 3, 2, 3, 11, 1,
    7, 4, 1, 2, 1, 3, 17, 1, 9, 1, 24, 1, 1, 4, 2, 2, 4, 1, 2, 7, 1, 1, 1, 3, 1, 2, 2, 4, 15, 1, 1,
    2, 1, 1, 2, 1, 5, 2, 5, 20, 2, 5, 9, 1, 10, 8, 7, 6, 1, 1, 1, 1, 1, 1, 6, 2, 1, 2, 8, 1, 1, 1,
    1, 5, 1, 1, 3, 1, 1, 1, 1, 3, 1, 1, 12, 4, 1, 3, 1, 1, 1, 1, 1, 10, 3, 1, 7, 5, 13, 1, 2, 3, 4,
    6, 1, 1, 30, 2, 9, 9, 1, 15, 38, 11, 3, 1, 8, 24, 7, 1, 9, 8, 10, 2, 1, 9, 31, 2, 13, 6, 2, 9,
    4, 49, 5, 2, 15, 2, 1, 10, 2, 1, 1, 1, 2, 2, 6, 15, 30, 35, 3, 14, 18, 8, 1, 16, 10, 28, 12,
    19, 45, 38, 1, 3, 2, 3, 13, 2, 1, 7, 3, 6, 5, 3, 4, 3, 1, 5, 7, 8, 1, 5, 3, 18, 5, 3, 6, 1, 21,
    4, 24, 9, 24, 40, 3, 14, 3, 21, 3, 2, 1, 2, 4, 2, 3, 1, 15, 15, 6, 5, 1, 1, 3, 1, 5, 6, 1, 9,
    7, 3, 3, 2, 1, 4, 3, 8, 21, 5, 16, 4, 5, 2, 10, 11, 11, 3, 6, 3, 2, 9, 3, 6, 13, 1, 2, 1, 1, 1,
    1, 11, 12, 6, 6, 1, 4, 2, 6, 5, 2, 1, 1, 3, 3, 6, 13, 3, 1, 1, 5, 1, 2, 3, 3, 14, 2, 1, 2, 2,
    2, 5, 1, 9, 5, 1, 1, 6, 12, 3, 12, 3, 4, 13, 2, 14, 2, 8, 1, 17, 5, 1, 16, 4, 2, 2, 21, 8, 9,
    6, 23, 20, 12, 25, 19, 9, 38, 8, 3, 21, 40, 25, 33, 13, 4, 3, 1, 4, 1, 2, 4, 1, 2, 5, 26, 2, 1,
    1, 2, 1, 3, 6, 2, 1, 1, 1, 1, 1, 1, 2, 3, 1, 1, 1, 9, 2, 3, 1, 1, 1, 3, 6, 3, 2, 1, 1, 6, 6, 1,
    8, 2, 2, 2, 1, 4, 1, 2, 3, 2, 7, 3, 2, 4, 1, 2, 1, 2, 2, 1, 1, 1, 1, 1, 3, 1, 2, 5, 4, 10, 9,
    4, 9, 1, 1, 1, 1, 1, 1, 5, 3, 2, 1, 6, 4, 9, 6, 1, 10, 2, 31, 17, 8, 3, 7, 5, 40, 1, 7, 7, 1,
    6, 5, 2, 10, 7, 8, 4, 15, 39, 25, 6, 28, 47, 18, 10, 7, 1, 3, 1, 1, 2, 1, 1, 1, 3, 3, 3, 1, 1,
    1, 3, 4, 2, 1, 4, 1, 3, 6, 10, 7, 8, 6, 2, 2, 1, 3, 3, 2, 5, 8, 7, 9, 12, 2, 15, 1, 1, 4, 1, 2,
    1, 1, 1, 3, 2, 1, 3, 3, 5, 6, 2, 3, 2, 10, 1, 4, 2, 8, 1, 1, 1, 11, 6, 1, 21, 4, 16, 3, 1, 3,
    1, 4, 2, 3, 6, 5, 1, 3, 1, 1, 3, 3, 4, 6, 1, 1, 10, 4, 2, 7, 10, 4, 7, 4, 2, 9, 4, 3, 1, 1, 1,
    4, 1, 8, 3, 4, 1, 3, 1, 6, 1, 4, 2, 1, 4, 7, 2, 1, 8, 1, 4, 5, 1, 1, 2, 2, 4, 6, 2, 7, 1, 10,
    1, 1, 3, 4, 11, 10, 8, 21, 4, 6, 1, 3, 5, 2, 1, 2, 28, 5, 5, 2, 3, 13, 1, 2, 3, 1, 4, 2, 1, 5,
    20, 3, 8, 11, 1, 3, 3, 3, 1, 8, 10, 9, 2, 10, 9, 2, 3, 1, 1, 2, 4, 1, 8, 3, 6, 1, 7, 8, 6, 11,
    1, 4, 29, 8, 4, 3, 1, 2, 7, 13, 1, 4, 1, 6, 2, 6, 12, 12, 2, 20, 3, 2, 3, 6, 4, 8, 9, 2, 7, 34,
    5, 1, 18, 6, 1, 1, 4, 4, 5, 7, 9, 1, 2, 2, 4, 3, 4, 1, 7, 2, 2, 2, 6, 2, 3, 25, 5, 3, 6, 1, 4,
    6, 7, 4, 2, 1, 4, 2, 13, 6, 4, 4, 3, 1, 5, 3, 4, 4, 3, 2, 1, 1, 4, 1, 2, 1, 1, 3, 1, 11, 1, 6,
    3, 1, 7, 3, 6, 2, 8, 8, 6, 9, 3, 4, 11, 3, 2, 10, 12, 2, 5, 11, 1, 6, 4, 5, 3, 1, 8, 5, 4, 6,
    6, 3, 5, 1, 1, 3, 2, 1, 2, 2, 6, 17, 12, 1, 10, 1, 6, 12, 1, 6, 6, 19, 9, 6, 16, 1, 13, 4, 4,
    15, 7, 17, 6, 11, 9, 15, 12, 6, 7, 2, 1, 2, 2, 15, 9, 3, 21, 4, 6, 49, 18, 7, 3, 2, 3, 1, 6, 8,
    2, 2, 6, 2, 9, 1, 3, 6, 4, 4, 1, 2, 16, 2, 5, 2, 1, 6, 2, 3, 5, 3, 1, 2, 5, 1, 2, 1, 9, 3, 1,
    8, 6, 4, 8, 11, 3, 1, 1, 1, 1, 3, 1, 13, 8, 4, 1, 3, 2, 2, 1, 4, 1, 11, 1, 5, 2, 1, 5, 2, 5, 8,
    6, 1, 1, 7, 4, 3, 8, 3, 2, 7, 2, 1, 5, 1, 5, 2, 4, 7, 6, 2, 8, 5, 1, 11, 4, 5, 3, 6, 18, 1, 2,
    13, 3, 3, 1, 21, 1, 1, 4, 1, 4, 1, 1, 1, 8, 1, 2, 2, 7, 1, 2, 4, 2, 2, 9, 2, 1, 1, 1, 4, 3, 6,
    3, 12, 5, 1, 1, 1, 5, 6, 3, 2, 4, 8, 2, 2, 4, 2, 7, 1, 8, 9, 5, 2, 3, 2, 1, 3, 2, 13, 7, 14, 6,
    5, 1, 1, 2, 1, 4, 2, 23, 2, 1, 1, 6, 3, 1, 4, 1, 15, 3, 1, 7, 3, 9, 14, 1, 3, 1, 4, 1, 1, 5, 8,
    1, 3, 8, 3, 8, 15, 11, 4, 14, 4, 4, 2, 5, 5, 1, 7, 1, 6, 14, 7, 7, 8, 5, 15, 4, 8, 6, 5, 6, 2,
    1, 13, 1, 20, 15, 11, 9, 2, 5, 6, 2, 11, 2, 6, 2, 5, 1, 5, 8, 4, 13, 19, 25, 4, 1, 1, 11, 1,
    34, 2, 5, 9, 14, 6, 2, 2, 6, 1, 1, 14, 1, 3, 14, 13, 1, 6, 12, 21, 14, 14, 6, 32, 17, 8, 32, 9,
    28, 1, 2, 4, 11, 8, 3, 1, 14, 2, 5, 15, 1, 1, 1, 1, 3, 6, 4, 1, 3, 4, 11, 3, 1, 1, 11, 30, 1,
    5, 1, 4, 1, 5, 8, 1, 1, 3, 2, 4, 3, 17, 35, 2, 6, 12, 17, 3, 1, 6, 2, 1, 1, 12, 2, 7, 3, 3, 2,
    1, 16, 2, 8, 3, 6, 5, 4, 7, 3, 3, 8, 1, 9, 8, 5, 1, 2, 1, 3, 2, 8, 1, 2, 9, 12, 1, 1, 2, 3, 8,
    3, 24, 12, 4, 3, 7, 5, 8, 3, 3, 3, 3, 3, 3, 1, 23, 10, 3, 1, 2, 2, 6, 3, 1, 16, 1, 16, 22, 3,
    10, 4, 11, 6, 9, 7, 7, 3, 6, 2, 2, 2, 4, 10, 2, 1, 1, 2, 8, 7, 1, 6, 4, 1, 3, 3, 3, 5, 10, 12,
    12, 2, 3, 12, 8, 15, 1, 1, 16, 6, 6, 1, 5, 9, 11, 4, 11, 4, 2, 6, 12, 1, 17, 5, 13, 1, 4, 9, 5,
    1, 11, 2, 1, 8, 1, 5, 7, 28, 8, 3, 5, 10, 2, 17, 3, 38, 22, 1, 2, 18, 12, 10, 4, 38, 18, 1, 4,
    44, 19, 4, 1, 8, 4, 1, 12, 1, 4, 31, 12, 1, 14, 7, 75, 7, 5, 10, 6, 6, 13, 3, 2, 11, 11, 3, 2,
    5, 28, 15, 6, 18, 18, 5, 6, 4, 3, 16, 1, 7, 18, 7, 36, 3, 5, 3, 1, 7, 1, 9, 1, 10, 7, 2, 4, 2,
    6, 2, 9, 7, 4, 3, 32, 12, 3, 7, 10, 2, 23, 16, 3, 1, 12, 3, 31, 4, 11, 1, 3, 8, 9, 5, 1, 30,
    15, 6, 12, 3, 2, 2, 11, 19, 9, 14, 2, 6, 2, 3, 19, 13, 17, 5, 3, 3, 25, 3, 14, 1, 1, 1, 36, 1,
    3, 2, 19, 3, 13, 36, 9, 13, 31, 6, 4, 16, 34, 2, 5, 4, 2, 3, 3, 5, 1, 1, 1, 4, 3, 1, 17, 3, 2,
    3, 5, 3, 1, 3, 2, 3, 5, 6, 3, 12, 11, 1, 3, 1, 2, 26, 7, 12, 7, 2, 14, 3, 3, 7, 7, 11, 25, 25,
    28, 16, 4, 36, 1, 2, 1, 6, 2, 1, 9, 3, 27, 17, 4, 3, 4, 13, 4, 1, 3, 2, 2, 1, 10, 4, 2, 4, 6,
    3, 8, 2, 1, 18, 1, 1, 24, 2, 2, 4, 33, 2, 3, 63, 7, 1, 6, 40, 7, 3, 4, 4, 2, 4, 15, 18, 1, 16,
    1, 1, 11, 2, 41, 14, 1, 3, 18, 13, 3, 2, 4, 16, 2, 17, 7, 15, 24, 7, 18, 13, 44, 2, 2, 3, 6, 1,
    1, 7, 5, 1, 7, 1, 4, 3, 3, 5, 10, 8, 2, 3, 1, 8, 1, 1, 27, 4, 2, 1, 12, 1, 2, 1, 10, 6, 1, 6,
    7, 5, 2, 3, 7, 11, 5, 11, 3, 6, 6, 2, 3, 15, 4, 9, 1, 1, 2, 1, 2, 11, 2, 8, 12, 8, 5, 4, 2, 3,
    1, 5, 2, 2, 1, 14, 1, 12, 11, 4, 1, 11, 17, 17, 4, 3, 2, 5, 5, 7, 3, 1, 5, 9, 9, 8, 2, 5, 6, 6,
    13, 13, 2, 1, 2, 6, 1, 2, 2, 49, 4, 9, 1, 2, 10, 16, 7, 8, 4, 3, 2, 23, 4, 58, 3, 29, 1, 14,
    19, 19, 11, 11, 2, 7, 5, 1, 3, 4, 6, 2, 18, 5, 12, 12, 17, 17, 3, 3, 2, 4, 1, 6, 2, 3, 4, 3, 1,
    1, 1, 1, 5, 1, 1, 9, 1, 3, 1, 3, 6, 1, 8, 1, 1, 2, 6, 4, 14, 3, 1, 4, 11, 4, 1, 3, 32, 1, 2, 4,
    13, 4, 1, 2, 4, 2, 1, 3, 1, 11, 1, 4, 2, 1, 4, 4, 6, 3, 5, 1, 6, 5, 7, 6, 3, 23, 3, 5, 3, 5, 3,
    3, 13, 3, 9, 10, 1, 12, 10, 2, 3, 18, 13, 7, 160, 52, 4, 2, 2, 3, 2, 14, 5, 4, 12, 4, 6, 4, 1,
    20, 4, 11, 6, 2, 12, 27, 1, 4, 1, 2, 2, 7, 4, 5, 2, 28, 3, 7, 25, 8, 3, 19, 3, 6, 10, 2, 2, 1,
    10, 2, 5, 4, 1, 3, 4, 1, 5, 3, 2, 6, 9, 3, 6, 2, 16, 3, 3, 16, 4, 5, 5, 3, 2, 1, 2, 16, 15, 8,
    2, 6, 21, 2, 4, 1, 22, 5, 8, 1, 1, 21, 11, 2, 1, 11, 11, 19, 13, 12, 4, 2, 3, 2, 3, 6, 1, 8,
    11, 1, 4, 2, 9, 5, 2, 1, 11, 2, 9, 1, 1, 2, 14, 31, 9, 3, 4, 21, 14, 4, 8, 1, 7, 2, 2, 2, 5, 1,
    4, 20, 3, 3, 4, 10, 1, 11, 9, 8, 2, 1, 4, 5, 14, 12, 14, 2, 17, 9, 6, 31, 4, 14, 1, 20, 13, 26,
    5, 2, 7, 3, 6, 13, 2, 4, 2, 19, 6, 2, 2, 18, 9, 3, 5, 12, 12, 14, 4, 6, 2, 3, 6, 9, 5, 22, 4,
    5, 25, 6, 4, 8, 5, 2, 6, 27, 2, 35, 2, 16, 3, 7, 8, 8, 6, 6, 5, 9, 17, 2, 20, 6, 19, 2, 13, 3,
    1, 1, 1, 4, 17, 12, 2, 14, 7, 1, 4, 18, 12, 38, 33, 2, 10, 1, 1, 2, 13, 14, 17, 11, 50, 6, 33,
    20, 26, 74, 16, 23, 45, 50, 13, 38, 33, 6, 6, 7, 4, 4, 2, 1, 3, 2, 5, 8, 7, 8, 9, 3, 11, 21, 9,
    13, 1, 3, 10, 6, 7, 1, 2, 2, 18, 5, 5, 1, 9, 9, 2, 68, 9, 19, 13, 2, 5, 1, 4, 4, 7, 4, 13, 3,
    9, 10, 21, 17, 3, 26, 2, 1, 5, 2, 4, 5, 4, 1, 7, 4, 7, 3, 4, 2, 1, 6, 1, 1, 20, 4, 1, 9, 2, 2,
    1, 3, 3, 2, 3, 2, 1, 1, 1, 20, 2, 3, 1, 6, 2, 3, 6, 2, 4, 8, 1, 3, 2, 10, 3, 5, 3, 4, 4, 3, 4,
    16, 1, 6, 1, 10, 2, 4, 2, 1, 1, 2, 10, 11, 2, 2, 3, 1, 24, 31, 4, 10, 10, 2, 5, 12, 16, 164,
    15, 4, 16, 7, 9, 15, 19, 17, 1, 2, 1, 1, 5, 1, 1, 1, 1, 1, 3, 1, 4, 3, 1, 3, 1, 3, 1, 2, 1, 1,
    3, 3, 7, 2, 8, 1, 2, 2, 2, 1, 3, 4, 3, 7, 8, 12, 92, 2, 10, 3, 1, 3, 14, 5, 25, 16, 42, 4, 7,
    7, 4, 2, 21, 5, 27, 26, 27, 21, 25, 30, 31, 2, 1, 5, 13, 3, 22, 5, 6, 6, 11, 9, 12, 1, 5, 9, 7,
    5, 5, 22, 60, 3, 5, 13, 1, 1, 8, 1, 1, 3, 3, 2, 1, 9, 3, 3, 18, 4, 1, 2, 3, 7, 6, 3, 1, 2, 3,
    9, 1, 3, 1, 3, 2, 1, 3, 1, 1, 1, 2, 1, 11, 3, 1, 6, 9, 1, 3, 2, 3, 1, 2, 1, 5, 1, 1, 4, 3, 4,
    1, 2, 2, 4, 4, 1, 7, 2, 1, 2, 2, 3, 5, 13, 18, 3, 4, 14, 9, 9, 4, 16, 3, 7, 5, 8, 2, 6, 48, 28,
    3, 1, 1, 4, 2, 14, 8, 2, 9, 2, 1, 15, 2, 4, 3, 2, 10, 16, 12, 8, 7, 1, 1, 3, 1, 1, 1, 2, 7, 4,
    1, 6, 4, 38, 39, 16, 23, 7, 15, 15, 3, 2, 12, 7, 21, 37, 27, 6, 5, 4, 8, 2, 10, 8, 8, 6, 5, 1,
    2, 1, 3, 24, 1, 16, 17, 9, 23, 10, 17, 6, 1, 51, 55, 44, 13, 294, 9, 3, 6, 2, 4, 2, 2, 15, 1,
    1, 1, 13, 21, 17, 68, 14, 8, 9, 4, 1, 4, 9, 3, 11, 7, 1, 1, 1, 5, 6, 3, 2, 1, 1, 1, 2, 3, 8, 1,
    2, 2, 4, 1, 5, 5, 2, 1, 4, 3, 7, 13, 4, 1, 4, 1, 3, 1, 1, 1, 5, 5, 10, 1, 6, 1, 5, 2, 1, 5, 2,
    4, 1, 4, 5, 7, 3, 18, 2, 9, 11, 32, 4, 3, 3, 2, 4, 7, 11, 16, 9, 11, 8, 13, 38, 32, 8, 4, 2, 1,
    1, 2, 1, 2, 4, 4, 1, 1, 1, 4, 1, 21, 3, 11, 1, 16, 1, 1, 6, 1, 3, 2, 4, 9, 8, 57, 7, 44, 1, 3,
    3, 13, 3, 10, 1, 1, 7, 5, 2, 7, 21, 47, 63, 3, 15, 4, 7, 1, 16, 1, 1, 2, 8, 2, 3, 42, 15, 4, 1,
    29, 7, 22, 10, 3, 78, 16, 12, 20, 18, 4, 67, 11, 5, 1, 3, 15, 6, 21, 31, 32, 27, 18, 13, 71,
    35, 5, 142, 4, 10, 1, 2, 50, 19, 33, 16, 35, 37, 16, 19, 27, 7, 1, 133, 19, 1, 4, 8, 7, 20, 1,
    4, 4, 1, 10, 3, 1, 6, 1, 2, 51, 5, 40, 15, 24, 43, 22928, 11, 1, 13, 154, 70, 3, 1, 1, 7, 4,
    10, 1, 2, 1, 1, 2, 1, 2, 1, 2, 2, 1, 1, 2, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 2, 1, 1, 1, 3, 2, 1, 1, 1, 1, 2, 1, 1,
];

const BASIC_RANGES_UTF8: &[u32] = &[
    0x0020, 0x00FF, // Basic Latin + Latin Supplement
    0x0100, 0x017F, // Latin Extended-A
    0x2000, 0x206F, // General Punctuation
    0x3000, 0x30FF, // CJK Symbols and Punctuations, Hiragana, Katakana
    0x31F0, 0x31FF, // Katakana Phonetic Extensions
    0xFF00, 0xFFEF, // Half-width characters
    0xFFFD, 0xFFFD, // Invalid
];

const FONT_GLYPH_RANGE_BUFFER_SZ: usize = 16384;

const SARASA_MONO_J_REGULAR: &[u8] = include_bytes!("../../res/sarasa-mono-j-regular.ttf.gz");

fn decompress_gzip_font(font_data: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(font_data);
    let mut font_buf = vec![];
    decoder.read_to_end(&mut font_buf).unwrap();
    font_buf
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum TextStyle {
    Kanji,
    Body,
}

pub struct Env {
    font_data: Vec<u8>,
    fonts: HashMap<TextStyle, FontId>,

    added_font_glyphs: HashSet<u32>,
    font_glyph_ranges: Vec<u32>,
    font_glyph_range_size: usize,
    font_atlas_dirty: bool,
}
impl Env {
    pub fn new() -> Self {
        let mut font_glyph_ranges = vec![0; FONT_GLYPH_RANGE_BUFFER_SZ];
        font_glyph_ranges[0..BASIC_RANGES_UTF8.len()].copy_from_slice(BASIC_RANGES_UTF8);

        let mut env = Env {
            font_data: decompress_gzip_font(SARASA_MONO_J_REGULAR),
            fonts: HashMap::new(),

            added_font_glyphs: HashSet::new(),
            font_glyph_ranges,
            font_glyph_range_size: BASIC_RANGES_UTF8.len(),
            font_atlas_dirty: true,
        };
        env.add_default_glyphs();
        env
    }
    pub fn font_atlas_dirty(&self) -> bool {
        self.font_atlas_dirty
    }
    fn add_default_glyphs(&mut self) {
        let mut code: u32 = 0x4e00;
        for off in JA_ACC_OFF_0X4_E00_UTF8 {
            code += *off as u32;
            self.add_font_glyph(code);
        }
    }
    fn add_font_glyph(&mut self, code: u32) {
        debug_assert!(!self.has_font_glyph(code));
        self.added_font_glyphs.insert(code);
        self.font_glyph_ranges[self.font_glyph_range_size] = code;
        self.font_glyph_ranges[self.font_glyph_range_size + 1] = code;
        self.font_glyph_range_size += 2;
        self.font_atlas_dirty = true;
    }
    fn has_font_glyph(&self, code: u32) -> bool {
        self.added_font_glyphs.contains(&code)
    }
    unsafe fn get_font_glyph_ranges(&mut self) -> &'static mut [u32] {
        let font_glyph_ranges = &mut self.font_glyph_ranges[0..self.font_glyph_range_size + 1];
        // /!\ DANGER /!\
        // Env will always outlive the ImGui Context, so this is safe.
        std::mem::transmute(font_glyph_ranges)
    }
    fn add_font(&mut self, style: TextStyle, font_id: FontId) {
        self.fonts.insert(style, font_id);
    }
    pub fn get_font(&self, style: TextStyle) -> FontId {
        *self.fonts.get(&style).unwrap()
    }
    pub fn update_fonts(&mut self, imgui: &mut imgui::Context, platform: &WinitPlatform) -> bool {
        if !self.font_atlas_dirty {
            return false;
        }

        imgui.fonts().clear();

        let hidpi_factor = platform.hidpi_factor();
        imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

        let ext_font_config = [FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::from_slice(unsafe { self.get_font_glyph_ranges() }),
            oversample_h: 3,
            ..Default::default()
        }];

        let mut create_font =
            |name: &str, font_data: &[u8], size_pt: f64, config: &[FontConfig]| {
                let font_sources: Vec<_> = config
                    .iter()
                    .map(|config| FontSource::TtfData {
                        data: font_data,
                        size_pixels: (size_pt * hidpi_factor) as f32,
                        config: Some(FontConfig {
                            name: Some(name.to_string()),
                            ..config.clone()
                        }),
                    })
                    .collect();
                imgui.fonts().add_font(font_sources.as_slice())
            };

        self.add_font(
            TextStyle::Body,
            create_font("Body", &self.font_data, 18.0, &ext_font_config),
        );
        self.add_font(
            TextStyle::Kanji,
            create_font("Kanji", &self.font_data, 48.0, &ext_font_config),
        );

        self.font_atlas_dirty = false;
        true
    }
    fn add_unknown_glyphs<T: AsRef<str>>(&mut self, text: T) {
        let text = text.as_ref();
        for c in text.chars() {
            if charset::is_kanji(&c) {
                let code = c as u32;
                if !self.has_font_glyph(code) {
                    self.add_font_glyph(code);
                }
            }
        }
    }
    pub fn add_unknown_glyphs_from_root(&mut self, root: &Root) {
        struct RootVisitor<'a>(&'a mut Env);
        impl<'a> RootVisitor<'a> {
            fn visit_conj(&mut self, conj: &Conjugation) {
                if let Some(reading) = conj.reading() {
                    self.0.add_unknown_glyphs(reading);
                }
            }
            fn visit_meta(&mut self, meta: &Meta) {
                self.0.add_unknown_glyphs(meta.text());
            }
            fn visit_word(&mut self, word: &Word) {
                match word {
                    Word::Plain(plain) => {
                        self.visit_meta(plain.meta());
                        plain.conj().iter().for_each(|x| self.visit_conj(x));
                    }
                    Word::Compound(compound) => {
                        self.visit_meta(compound.meta());
                        compound
                            .components()
                            .iter()
                            .for_each(|x| self.visit_term(x))
                    }
                }
            }
            fn visit_term(&mut self, term: &Term) {
                match term {
                    Term::Word(word) => {
                        self.visit_word(word);
                    }
                    Term::Alternative(alt) => {
                        alt.alts().iter().for_each(|x| self.visit_word(x));
                    }
                }
            }

            fn visit_clause(&mut self, clause: &Clause) {
                clause
                    .romanized()
                    .iter()
                    .map(|x| x.term())
                    .for_each(|x| self.visit_term(x));
            }
            fn visit_segment(&mut self, segment: &Segment) {
                if let Segment::Clauses(clauses) = &segment {
                    clauses.iter().for_each(|x| self.visit_clause(x))
                }
            }
            pub fn visit_root(&mut self, root: &Root) {
                root.segments().iter().for_each(|x| self.visit_segment(x));
            }
        }
        let num_glyphs = self.added_font_glyphs.len();

        let mut root_visitor = RootVisitor(self);
        root_visitor.visit_root(root);

        let delta = self.added_font_glyphs.len() - num_glyphs;
        if delta > 0 {
            log::info!("added {} font glyphs", delta);
        }
    }
}