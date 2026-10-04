/** Bitcoin's base58 alphabet, which Solana addresses use. */
const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

/** Encodes bytes as base58: each leading zero byte as a `1`, the rest as one big number. */
export function encodeBase58(bytes: Uint8Array): string {
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  let digits = '';
  while (value > 0n) {
    digits = ALPHABET[Number(value % 58n)]! + digits;
    value /= 58n;
  }
  let zeros = 0;
  while (zeros < bytes.length && bytes[zeros] === 0) zeros += 1;
  return '1'.repeat(zeros) + digits;
}

/** Decodes base58 text, throwing a `TypeError` for a character outside the alphabet. */
export function decodeBase58(text: string): Uint8Array<ArrayBuffer> {
  let value = 0n;
  for (const character of text) {
    const digit = ALPHABET.indexOf(character);
    if (digit < 0) throw new TypeError(`${JSON.stringify(text)} is not base58`);
    value = value * 58n + BigInt(digit);
  }
  const bytes: number[] = [];
  while (value > 0n) {
    bytes.unshift(Number(value & 0xffn));
    value >>= 8n;
  }
  let ones = 0;
  while (ones < text.length && text[ones] === '1') ones += 1;
  return Uint8Array.from([...new Array<number>(ones).fill(0), ...bytes]);
}
