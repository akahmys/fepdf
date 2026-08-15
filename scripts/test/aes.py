"""AES-128 and AES-256 in ECB and CBC, in pure Python, for building test fixtures.

Deliberately independent of the engine. `make_encrypted.py` uses this to produce
AES-256 encrypted PDFs that fepdf must then read; generating them with fepdf's own
cryptography would test it against itself. Python has no AES in its standard library
and this project does not add dependencies for test tooling, so it is written out.

Correctness is checked against the FIPS-197 appendix C vectors in `self_test`, which
`make_encrypted.py` runs before it generates anything.

Not constant-time, and not for protecting anything. It exists to make ciphertext that a
conforming reader must be able to undo.
"""

from __future__ import annotations

SBOX = bytes.fromhex(
    "637c777bf26b6fc53001672bfed7ab76ca82c97dfa5947f0add4a2af9ca472c0"
    "b7fd9326363ff7cc34a5e5f171d8311504c723c31896059a071280e2eb27b275"
    "09832c1a1b6e5aa0523bd6b329e32f8453d100ed20fcb15b6acbbe394a4c58cf"
    "d0efaafb434d338545f9027f503c9fa851a3408f929d38f5bcb6da2110fff3d2"
    "cd0c13ec5f974417c4a77e3d645d197360814fdc222a908846eeb814de5e0bdb"
    "e0323a0a4906245cc2d3ac629195e479e7c8376d8dd54ea96c56f4ea657aae08"
    "ba78252e1ca6b4c6e8dd741f4bbd8b8a703eb5664803f60e613557b986c11d9e"
    "e1f8981169d98e949b1e87e9ce5528df8ca1890dbfe6426841992d0fb054bb16"
)
RCON = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36, 0x6C, 0xD8, 0xAB, 0x4D]


def _xtime(a: int) -> int:
    a <<= 1
    return (a ^ 0x1B) & 0xFF if a & 0x100 else a


def _mul(a: int, b: int) -> int:
    out = 0
    for _ in range(8):
        if b & 1:
            out ^= a
        b >>= 1
        a = _xtime(a)
    return out


def _expand_key(key: bytes) -> list[list[int]]:
    nk = len(key) // 4
    rounds = nk + 6
    words = [list(key[4 * i : 4 * i + 4]) for i in range(nk)]
    for i in range(nk, 4 * (rounds + 1)):
        temp = list(words[i - 1])
        if i % nk == 0:
            temp = temp[1:] + temp[:1]
            temp = [SBOX[b] for b in temp]
            temp[0] ^= RCON[i // nk - 1]
        elif nk > 6 and i % nk == 4:
            temp = [SBOX[b] for b in temp]
        words.append([words[i - nk][j] ^ temp[j] for j in range(4)])
    return words


def _encrypt_block(block: bytes, words: list[list[int]], rounds: int) -> bytes:
    state = [list(block[i::4]) for i in range(4)]  # row-major

    def add_round_key(rnd: int) -> None:
        for c in range(4):
            for r in range(4):
                state[r][c] ^= words[rnd * 4 + c][r]

    add_round_key(0)
    for rnd in range(1, rounds + 1):
        for r in range(4):
            for c in range(4):
                state[r][c] = SBOX[state[r][c]]
        for r in range(1, 4):
            state[r] = state[r][r:] + state[r][:r]
        if rnd != rounds:
            for c in range(4):
                col = [state[r][c] for r in range(4)]
                state[0][c] = _mul(col[0], 2) ^ _mul(col[1], 3) ^ col[2] ^ col[3]
                state[1][c] = col[0] ^ _mul(col[1], 2) ^ _mul(col[2], 3) ^ col[3]
                state[2][c] = col[0] ^ col[1] ^ _mul(col[2], 2) ^ _mul(col[3], 3)
                state[3][c] = _mul(col[0], 3) ^ col[1] ^ col[2] ^ _mul(col[3], 2)
        add_round_key(rnd)

    return bytes(state[r][c] for c in range(4) for r in range(4))


def ecb_encrypt(key: bytes, data: bytes) -> bytes:
    """Encrypts whole blocks. `data` must be a multiple of 16 bytes."""
    if len(data) % 16:
        raise ValueError("ECB input must be a multiple of the block size")
    words = _expand_key(key)
    rounds = len(key) // 4 + 6
    return b"".join(_encrypt_block(data[i : i + 16], words, rounds) for i in range(0, len(data), 16))


def cbc_encrypt(key: bytes, iv: bytes, data: bytes) -> bytes:
    """Encrypts whole blocks in CBC mode, adding no padding of its own."""
    if len(data) % 16:
        raise ValueError("CBC input must be a multiple of the block size")
    words = _expand_key(key)
    rounds = len(key) // 4 + 6
    out = bytearray()
    prev = iv
    for i in range(0, len(data), 16):
        block = bytes(a ^ b for a, b in zip(data[i : i + 16], prev))
        prev = _encrypt_block(block, words, rounds)
        out.extend(prev)
    return bytes(out)


def pkcs7(data: bytes) -> bytes:
    pad = 16 - (len(data) % 16)
    return data + bytes([pad]) * pad


def self_test() -> None:
    """FIPS-197 appendix C. A wrong AES here would produce fixtures nothing can read."""
    key128 = bytes(range(16))
    plain = bytes.fromhex("00112233445566778899aabbccddeeff")
    assert ecb_encrypt(key128, plain).hex() == "69c4e0d86a7b0430d8cdb78070b4c55a", "AES-128"

    key256 = bytes(range(32))
    assert ecb_encrypt(key256, plain).hex() == "8ea2b7ca516745bfeafc49904b496089", "AES-256"

    # SP 800-38A F.2.5, the first CBC-AES256 block.
    cbc_key = bytes.fromhex(
        "603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4"
    )
    iv = bytes.fromhex("000102030405060708090a0b0c0d0e0f")
    block = bytes.fromhex("6bc1bee22e409f96e93d7e117393172a")
    assert cbc_encrypt(cbc_key, iv, block).hex() == "f58c4c04d6e5f1ba779eabfb5f7bfbd6", "CBC"


if __name__ == "__main__":
    self_test()
    print("AES self-test passed (FIPS-197 C.1/C.3, SP 800-38A F.2.5)")
