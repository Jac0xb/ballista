import {
  account,
  compileTemplate,
  defineTemplate,
  expression,
  planTemplateUpload,
  systemTransfer,
} from '../src/index.js';

const address = (byte: number) => new Uint8Array(32).fill(byte);

const transfer = defineTemplate({
  inputs: { lamports: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: address(1) },
    sender: { signer: true, writable: true },
    recipient: { writable: true },
  },
  steps: [
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('sender'),
      to: account.fixed('recipient'),
      lamports: expression.input('lamports'),
    }),
  ],
});

export const compiledTransfer = compileTemplate(transfer);
export const uploadPlan = planTemplateUpload(compiledTransfer, 0);
