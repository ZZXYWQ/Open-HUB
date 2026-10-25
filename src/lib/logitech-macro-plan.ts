import type {MultiStep} from './multi';
import type {NativeMacroSelection} from './logitech-macros';
import type {OnboardSnapshot} from './api';
export interface NativeMacroDefinition{id:string;name:string;mode:'Once';steps:MultiStep[]}
export interface NativeMacroRequest{selection:NativeMacroSelection;definition:NativeMacroDefinition}
export interface NativeMacroPreview{macro_sector:number;slot:number;profile_sector:number;button:number;byte_count:number;page_count:number}
export interface NativeMacroWriteResult{report:{preview:NativeMacroPreview;active_reloaded:boolean;reload_note:string|null};backup_path:string;snapshot:OnboardSnapshot|null;refresh_note:string|null}
