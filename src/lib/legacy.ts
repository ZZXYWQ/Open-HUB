import data from './legacy-models.json';
export type LegacyBrand='asus'|'roccat'|'gskill'|'etekcity'|'marsgaming';
export const legacyModels=data as Array<(typeof data)[number]&{brand:LegacyBrand}>;
export const legacyNames:Record<LegacyBrand,string>={asus:'华硕 / ASUS',roccat:'Roccat',gskill:'芝奇 / G.Skill',etekcity:'Etekcity',marsgaming:'Mars Gaming'};
export function legacyModel(v:number,p:number){return legacyModels.find(m=>m.vendor===v&&m.product===p);}
export function isLegacyBrand(s:string):s is LegacyBrand{return s in legacyNames;}
export function legacyKeyAllowed(brand:LegacyBrand,k:number){return brand==='asus'?(k>=4&&k<=0x2e)||[0x35,0x38,0x57].includes(k)||(k>=0x3a&&k<=0x45)||(k>=0x4a&&k<=0x4c)||(k>=0x4e&&k<=0x52)||(k>=0x59&&k<=0x61):brand==='gskill'?k>=4&&k<=0x6a:k>=4&&k<=0xa4;}
