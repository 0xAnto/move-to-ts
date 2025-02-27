import { HexString } from "aptos"
import { MoveStructTag } from "aptos/dist/generated";
import { assert } from "./utils";

export enum AtomicTypeTag{
  Bool = "bool",
  U8 = "u8",
  U64 = "u64",
  U128 = "u128",
  Address = "address",
  Signer = "signer", // only needed for script function interface
}

export class StructTag {
  constructor(
    public address: HexString,
    public module: string,
    public name: string,
    public typeParams: TypeTag[]
  ) {

  }
  getFullname(): string {
    const typeParamString = getTypeParamsString(this.typeParams);
    return `${this.address.hex()}::${this.module}::${this.name}${typeParamString}`;
  }

  getParamlessName(): string {
    return `${this.address.hex()}::${this.module}::${this.name}`;
  }

  getAptosMoveTypeTag(): MoveStructTag {
    return {
      address: this.address.toShortString(),
      module: this.module,
      name: this.name,
      generic_type_params: this.typeParams.map(getTypeTagFullname,)
    };
  }
}

export class VectorTag {
  constructor(
    public elementType: TypeTag
  ) {

  }
}

export class TypeParamIdx {
  constructor(
    public index: number,
  ) {

  }
}

export type TypeTag = AtomicTypeTag | VectorTag | StructTag | TypeParamIdx;

export function getTypeTagFullname(typeTag: TypeTag): string {
  if (typeTag instanceof VectorTag) {
    const vecTag = typeTag as VectorTag;
    return `vector<${getTypeTagFullname(vecTag.elementType)}>`
  }
  else if (typeTag instanceof StructTag) {
    const structTag = typeTag as StructTag;
    return structTag.getFullname();
  }
  else if (typeTag instanceof TypeParamIdx) {
    return `$tv${typeTag.index}`;
  }
  else {
    const atomicTag = typeTag as AtomicTypeTag;
    return atomicTag;
  }
}

export function getTypeTagParamlessName(typeTag: TypeTag): string {
  if (typeTag instanceof VectorTag) {
    return `vector`
  }
  else if (typeTag instanceof StructTag) {
    const structTag = typeTag as StructTag;
    return structTag.getParamlessName();
  }
  else if (typeTag instanceof TypeParamIdx) {
    return `$tv${typeTag.index}`;
  }
  else {
    const atomicTag = typeTag as AtomicTypeTag;
    return atomicTag;
  }
}

export function getTypeParamsString(typeParams: TypeTag[]) {
  if (typeParams.length === 0) {
    return "";
  }
  return `<${typeParams.map(getTypeTagFullname).join(', ')}>`
}

function splitByDoubleColon(name: string) {
  const endIdx = name.indexOf("::");
  assert(endIdx >= 0);
  assert(name.length > endIdx + 2);
  return [name.substr(0, endIdx), name.substr(endIdx+2)];
}

export function parseQualifiedStructTag(name: string): [(null | StructTag), string] {
  const isQualifiedStruct = name.includes("::");
  if (!isQualifiedStruct) {
    return [null, name];
  }
  const [address, withoutAddress] = splitByDoubleColon(name);
  const hexAddress = new HexString(address);
  const [module, withoutModule] = splitByDoubleColon(withoutAddress);
  // structName<...>
  if (withoutModule.includes("<")) {
    const leftBracketIdx = withoutModule.indexOf("<");
    const structName = withoutModule.substr(0, leftBracketIdx);
    const afterLeftBracket = withoutModule.substr(leftBracketIdx+1);
    const typeParams: TypeTag[] = [];
    let [result, remaining] = parseTypeTag(afterLeftBracket);
    while(true) {
      if (result === null) {
        throw new Error( `Badly formatted struct name: ${name}`);
      }
      typeParams.push(result);
      // consume the closing bracket
      if (remaining.startsWith('>')) {
        return [new StructTag(hexAddress, module, structName, typeParams), remaining.substr(1)];
      }
      // more params to parse
      else if (remaining.startsWith(', ')) {
        [result, remaining] = parseTypeTag(remaining.substr(2));
      }
      else if (remaining.startsWith(',')) {
        [result, remaining] = parseTypeTag(remaining.substr(1));
      }
      else {
        throw new Error( `Badly formatted struct name: ${name}`);
      }
    }
  }
  // just structName
  else {
    // structName could be followed by ',' or '>'
    const commaIdx = withoutModule.indexOf(',');
    const brackIdx = withoutModule.indexOf('>');
    if (commaIdx === -1 && brackIdx === -1) {
      // fully consumed
      return [new StructTag(hexAddress, module, withoutModule, []), ""];
    }
    const separatorIdx = commaIdx === -1 ? brackIdx : Math.min(commaIdx, brackIdx);
    return [new StructTag(hexAddress, module, withoutModule.substr(0, separatorIdx), []), withoutModule.substr(separatorIdx)];
  }
}

export function parseVectorTag(name: string): [(null|VectorTag), string] {
  if (!name.startsWith('vector<')) {
    return [null, name];
  }
  let [elementType, remaining] = parseTypeTag(name.substr(7));
  if (elementType === null || !remaining.startsWith('>')) {
    throw new Error(`Badly formatted vector type name: ${name}`);
  }
  // consume the remaining '>'
  return [new VectorTag(elementType), remaining.substr(1)];
}

export function parseAtomicTag(name: string): [(null|AtomicTypeTag), string] {
  const atomicTags = Object.values(AtomicTypeTag);
  for(const tag of atomicTags) {
    const value = tag;
    if (name.startsWith(value)) {
      // fully consumed the name
      if (name.length === value.length) {
        return [value, ""];
      } 
      // the name is an AtomicTypeTag followed by other template elements
      else if( [',', '>'].includes(name[value.length])) {
        return [value, name.substr(value.length)];
      }
      // miss
    }
  }
  return [null, name];
}

const CHAR_CODE_0 = '0'.charCodeAt(0);
const CHAR_CODE_9 = '9'.charCodeAt(0);

export function parseTypeParameter(name: string): [(null|TypeParamIdx), string] {
  if(!name.startsWith('$tv')) {
    return [null, name];
  }
  let idx = 3;
  for(; idx < name.length; idx++) {
    const charCode = name.charCodeAt(idx);
    if(charCode >= CHAR_CODE_0 && charCode <= CHAR_CODE_9) {
      continue;
    }
    break;
  }
  if(idx === 3) {
    throw new Error(`Failed to find number after $tv in :${name}`);
  }
  const paramIdx = parseInt(name.substr(3, idx - 3));
  return [new TypeParamIdx(paramIdx), name.substr(idx)];
}

export function parseTypeTag(name: string): [(null | TypeTag), string] {
  const [atomicResult, remaining1] = parseAtomicTag(name);
  if (atomicResult !== null) {
    return [atomicResult, remaining1];
  }
  const [vectorResult, remaining2] = parseVectorTag(name);
  if (vectorResult !== null) {
    return [vectorResult, remaining2];
  }
  const [structResult, remaining3] = parseQualifiedStructTag(name);
  if (structResult !== null) {
    return [structResult, remaining3];
  }
  const [tvResult, remaining4] = parseTypeParameter(name);
  if (tvResult !== null) {
    return [tvResult, remaining4];
  }
  throw new Error(`Bad typetag: ${name}`);
}

export function parseTypeTagOrThrow(name: string): TypeTag {
  const [tag, remaining] = parseTypeTag(name);
  if (!tag || remaining.length > 0) {
    throw new Error(`Invalid type tag: ${name}`);
  }
  return tag;
}

export function parseMoveStructTag(moveTag: MoveStructTag): StructTag {
  const params = moveTag.generic_type_params.map(parseTypeTagOrThrow);
  return new StructTag(new HexString(moveTag.address), moveTag.module, moveTag.name, params);
}

export function parseResourceType(fullname: string): StructTag {
  const [result, remaining] = parseQualifiedStructTag(fullname);
  if (result === null || remaining.length !== 0) {
    throw new Error(`Badly formatted resource type: ${fullname}`);
  }
  return result;
}

export function substituteTypeParams(toSubstitute: TypeTag, typeParams: TypeTag[]): TypeTag {
  if(toSubstitute instanceof StructTag) {
    let params = toSubstitute.typeParams.map(p=>substituteTypeParams(p, typeParams));
    return new StructTag(toSubstitute.address, toSubstitute.module, toSubstitute.name, params);
  }
  else if (toSubstitute instanceof VectorTag) {
    const innerSubbed = substituteTypeParams(toSubstitute.elementType, typeParams);
    return new VectorTag(innerSubbed);
  }
  else if (toSubstitute instanceof TypeParamIdx) {
    let subbed = typeParams[toSubstitute.index];
    if(!subbed) {
      throw new Error(`Did not find param ${toSubstitute.index} in ${JSON.stringify(typeParams)}`);
    }
    return subbed;
  }
  else {
    // AtomicTypeTag
    return toSubstitute;
  }
}

export function isTypeTagConcrete(tag: TypeTag): boolean {
  if(tag instanceof TypeParamIdx) {
    return false;
  }
  else if(tag instanceof StructTag) {
    // if all the parameters are filled with concret types instead of TypeParamIdx
    for(const tv of tag.typeParams) {
      if (!isTypeTagConcrete(tv)) {
        return false;
      }
    }
    return true;
  }
  else if( tag instanceof VectorTag) {
    return isTypeTagConcrete(tag.elementType);
  }
  else {
    // AtomicTypeTag
    return true;
  }
}