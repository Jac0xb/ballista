import path from 'path';
import {
  createFromRoot,
  pdaNode,
  addPdasVisitor,
  constantPdaSeedNodeFromString,
  variablePdaSeedNode,
  publicKeyTypeNode,
  numberTypeNode,
  bottomUpTransformerVisitor,
  definedTypeLinkNode,
  fillDefaultPdaSeedValuesVisitor,
  updateInstructionsVisitor,
  publicKeyValueNode,
  pdaValueNode,
  setInstructionAccountDefaultValuesVisitor,
  updateAccountsVisitor,
  pdaLinkNode,
  structTypeNode,
} from 'codama';
import fs from 'fs';
import { rootNodeFromAnchor } from '@codama/nodes-from-anchor';
import { renderJavaScriptVisitor, renderRustVisitor } from '@codama/renderers';

// Paths.
const clientDir = path.join(__dirname, '..', 'clients');
const programDir = path.join(__dirname, '..', 'programs', 'ballista');

const ballistaIdl = require(path.join(programDir, 'ballista.json'));

ballistaIdl.accounts = [
  {
    name: 'taskDefinition',
    data: [
      {
        name: 'id',
        type: 'u8',
      },
    ],
  },
];

// Instanciate Codama.
const codama = createFromRoot(rootNodeFromAnchor(ballistaIdl));

// // Memory account PDA
codama.update(
  addPdasVisitor({
    ballista: [
      pdaNode({
        name: 'taskDefinition',
        seeds: [
          constantPdaSeedNodeFromString('utf8', 'task-definition'),
          variablePdaSeedNode('payer', publicKeyTypeNode()),
          variablePdaSeedNode('taskId', numberTypeNode('u8')),
        ],
      }),
    ],
  }),
);

// codama.getRoot().program.pdas.push()

codama.update(
  setInstructionAccountDefaultValuesVisitor([
    // {
    //     // Set this public key as default value to any account named 'counterProgram'.
    //     account: 'counterProgram',
    //     defaultValue: publicKeyValueNode('MyCounterProgram11111111111111111111111111'),
    // },
    {
      // Set this PDA as default value to any account named 'associatedToken' or 'ata'.
      account: /^task/,
      defaultValue: pdaValueNode('taskDefinition'),
    },
  ]),
);

codama.update(
  updateAccountsVisitor({
    ballista: {
      name: 'taskDefinition',
      pda: pdaLinkNode('taskDefinition'),
      seeds: [
        constantPdaSeedNodeFromString('utf8', 'task-definition'),
        variablePdaSeedNode('payer', publicKeyTypeNode()),
        variablePdaSeedNode('id', numberTypeNode('u8')),
      ],
      // data: {
      //   id: numberTypeNode('u8'),
      // }
    },
  }),
);

// // How to set a default value for an account in an instruction.
codama.update(
  updateInstructionsVisitor({
    createTask: {
      accounts: {
        // systemProgram: {
        //   defaultValue: publicKeyValueNode('<pubkey>'),
        // },
        taskDefinition: {
          defaultValue: pdaValueNode('taskDefinition'),
        },
      },
    },
    // memoryWrite: {
    //   // accounts: {
    //   //   systemProgram: {
    //   //     defaultValue: k.publicKeyValueNode('<pubkey>'),
    //   //   },
    //   //   memoryAccount: {
    //   //     defaultValue: k.pdaValueNode('memory'),
    //   //   },
    //   // },
    //   arguments: {
    //     memoryId: {
    //       defaultValue: k.numberValueNode(0),
    //     },
    //   },
    // },
  }),
);

// codama.update(
//   k.bottomUpTransformerVisitor([
//     {
//       select: '[instructionArgumentNode]logLevel',
//       transform: (node) => {
//         return k.instructionArgumentNode({
//           ...node,
//           defaultValue: k.enumValueNode('logLevel', 'Silent'),
//         });
//       },
//     },
//   ])
// );

// for (const definedType of [
//   'value',
//   'expression',
//   'schema',
//   'taskDefinition',
// ]) {
//   codama.update(
//     bottomUpTransformerVisitor([
//       {
//         select: ['[definedTypeLinkNode]', definedType],
//         transform: (node) => {
//           return definedTypeLinkNode(node.kind, 'hooked');
//         },
//       },
//     ]),
//   );
// }

// How to log the codama tree
// codama.accept(k.consoleLogVisitor(k.getDebugStringVisitor({ indent: true })));

//
// Setting a default value for an instruction arg.
//
// codama.update(
//   k.setStructDefaultValuesVisitor({
//     memoryWrite: {
//       memory_id: k.numberValueNode(0),
//     },
//   })
// );

// // Render preview JavaScript.
// const previewJsDir = path.join(clientDir, 'preview-js', 'src', 'generated');
// const previewPrettier = require(
//   path.join(clientDir, 'preview-js', '.prettierrc.json')
// );
// codama.accept(
//   k.renderJavaScriptExperimentalVisitor(previewJsDir, {
//     prettier: previewPrettier,
//   })
// );

// codama.update(fillDefaultPdaSeedValuesVisitor(instructionNode, linkables, strictMode));

console.log(codama.getRoot());

// // Render preview JavaScript.
const jsDir = path.join(clientDir, 'js', 'src', 'generated');
// const prettier = require(path.join(clientDir, 'js', '.prettierrc.json'));
codama.accept(
  renderJavaScriptVisitor(jsDir, {
    // prettier
  }),
);

// Render Rust.
const crateDir = path.join(clientDir, 'rust');
const rustDir = path.join(clientDir, 'rust', 'src', 'generated');
codama.accept(
  renderRustVisitor(rustDir, {
    formatCode: true,
    crateFolder: crateDir,
    linkOverrides: {
      // pdas: {
      //   taskDefinition: 'hooked',
      // },
      definedTypes: {
        value: 'hooked',
        expression: 'hooked',
        taskDefinition: 'hooked',
      },
    },
  }),
);
