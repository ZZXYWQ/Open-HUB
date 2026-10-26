// Official mouseMacro translations + A7 V3 mode picker/encoder (pinned facts
// in docs/development/mchose-mode-state-official-facts.json). No repeat counts.
export const MCHOSE_MACRO_MODES=[
 {value:2,label:'执行一次',description:'按下后执行一次快捷指令'},
 {value:0,label:'按住时重复',description:'按住时重复执行，松开停止'},
 {value:1,label:'循环至再次按下',description:'再次按下相同按键时停止'},
 {value:3,label:'循环至其他按键按下',description:'按下其他按键时停止'},
] as const;
export const MCHOSE_NEW_MACRO_MODE=2;
