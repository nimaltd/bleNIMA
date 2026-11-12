// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addressKind,
  answerOf,
  client,
  config,
  describeMessage,
  faultText,
  modelName,
  oobInfoText,
  opcodeBytes,
  opcodeText,
  packIndexes,
  parseAddress,
  parseComposition,
  parseHex,
  parseSensorData,
  splitOpcode,
  transitionText,
  unpackIndexList,
  unpackIndexes,
} from "./mesh.ts";

const hex = (text: string) => parseHex(text) ?? [];
const asHex = (bytes: number[]) => bytes.map((b) => b.toString(16).padStart(2, "0")).join("");

test("opcodes of one, two and three bytes are told apart", () => {
  assert.deepEqual(splitOpcode(hex("0400000000")), { opcode: 0x04, params: [0, 0, 0, 0] });
  assert.deepEqual(splitOpcode(hex("820101")), { opcode: 0x8201, params: [1] });
  assert.deepEqual(splitOpcode(hex("c15900aa")), { opcode: 0xc15900, params: [0xaa] });
  assert.equal(splitOpcode(hex("7f")), null);
  assert.equal(splitOpcode(hex("82")), null);
  assert.deepEqual(opcodeBytes(0x02), [0x02]);
  assert.deepEqual(opcodeBytes(0x8201), [0x82, 0x01]);
  assert.deepEqual(opcodeBytes(0xc15900), [0xc1, 0x59, 0x00]);
  assert.equal(opcodeText(0x8201), "0x8201");
  assert.equal(opcodeText(0x04), "0x04");
});

test("Config AppKey Add is the spec's message 6", () => {
  const key = hex("63964771734fbd76e3b40519d1d94a48");
  assert.equal(asHex(config.appKeyAdd(0x456, 0x123, key)), "0056341263964771734fbd76e3b40519d1d94a48");
  assert.deepEqual(unpackIndexes(packIndexes(0x456, 0x123), 0), [0x456, 0x123]);
});

test("Config AppKey Status reads as the spec's message 16", () => {
  assert.deepEqual(describeMessage(hex("800300563412")), {
    name: "Config AppKey Status",
    text: "Success: NetKey 1110, AppKey 291",
  });
  assert.equal(describeMessage(hex("800306563412")).text, "Key Index Already Stored: NetKey 1110, AppKey 291");
});

test("Health Current Status reads as the spec's message 18", () => {
  assert.deepEqual(describeMessage(hex("0400000000")), {
    name: "Health Current Status",
    text: "test 0, company 0x0000, no faults",
  });
  assert.equal(describeMessage(hex("05005900010e")).text, "test 0, company 0x0059, Battery Low Warning, Overheat Error");
});

test("fault codes come in a warning and an error for each condition", () => {
  assert.equal(faultText(0x00), "No Fault");
  assert.equal(faultText(0x01), "Battery Low Warning");
  assert.equal(faultText(0x02), "Battery Low Error");
  assert.equal(faultText(0x31), "Mechanism Jammed Warning");
  assert.equal(faultText(0x32), "Mechanism Jammed Error");
  assert.equal(faultText(0x33), "fault 0x33");
  assert.equal(faultText(0x80), "vendor fault 0x80");
});

test("composition data page 0 is read element by element", () => {
  // Company 0x0059, product 1, version 2, 10 RPL entries, Relay and Proxy;
  // one element with three SIG models and one vendor model.
  const data = hex("5900 0100 0200 0a00 0300 0000 03 01 0000 0200 0010 59000100");
  const composition = parseComposition(data);
  assert.ok(composition);
  assert.equal(composition.cid, 0x0059);
  assert.equal(composition.crpl, 10);
  assert.equal(composition.features, 3);
  assert.deepEqual(composition.elements, [
    {
      location: 0,
      models: [
        { id: 0x0000, vendor: null },
        { id: 0x0002, vendor: null },
        { id: 0x1000, vendor: null },
        { id: 0x0001, vendor: 0x0059 },
      ],
    },
  ]);
  assert.equal(parseComposition(data.slice(0, -1)), null);
  assert.match(describeMessage([0x02, 0x00, ...data]).text, /1 elements, 4 models, features Relay, Proxy/);
  assert.equal(modelName({ id: 0x1000, vendor: null }), "Generic OnOff Server");
  assert.equal(modelName({ id: 0x0001, vendor: 0x0059 }), "Vendor model 0x0001 of 0x0059");
  assert.equal(modelName({ id: 0x1fff, vendor: null }), "SIG model 0x1FFF");
});

test("binding, publication and subscription messages carry SIG and vendor models", () => {
  const onOff = { id: 0x1000, vendor: null };
  const vendor = { id: 0x0001, vendor: 0x0059 };
  assert.equal(asHex(config.bind(0x0002, 0, onOff)), "803d020000000010");
  assert.equal(asHex(config.bind(0x0002, 1, vendor)), "803d0200010059000100");
  assert.equal(asHex(config.bindingsGet(0x0002, vendor)), "804d020059000100");
  assert.equal(asHex(config.subscribe(0x0002, 0xc000, onOff)), "801b020000c00010");
  assert.equal(asHex(config.subscriptionsGet(0x0002, onOff)), "8029020000 10".replace(" ", ""));
  const publication = {
    address: 0xc000,
    appKey: 1,
    credentials: false,
    ttl: 0xff,
    period: 0x0a,
    retransmitCount: 2,
    retransmitSteps: 3,
  };
  assert.equal(asHex(config.publicationSet(0x0002, onOff, publication)), "03020000c00100ff0a1a0010");
  assert.equal(
    describeMessage(hex("801900020000c00100ff0a1a0010")).text,
    "Success: element 0x0002, Generic OnOff Server, publishes to 0xC000 with AppKey 1, TTL default, every 1 s, 2 retransmissions 200 ms apart",
  );
  assert.equal(
    describeMessage(hex("803e000200000000 10".replace(" ", ""))).text,
    "Success: element 0x0002, Generic OnOff Server, AppKey 0",
  );
  assert.equal(
    describeMessage(hex("802a000200001000c001c0")).text,
    "Success: element 0x0002, Generic OnOff Server, subscribed to 0xC000, 0xC001",
  );
});

test("the messages bleNIMA sends read in words too", () => {
  const onOff = { id: 0x1000, vendor: null };
  assert.equal(describeMessage(config.compositionGet(0)).text, "page 0");
  assert.equal(describeMessage(config.bind(0x0002, 1, onOff)).text, "element 0x0002, Generic OnOff Server, AppKey 1");
  assert.equal(describeMessage(config.subscribe(0x0003, 0xc001, onOff)).text, "element 0x0003, Generic OnOff Server, address 0xC001");
  assert.equal(describeMessage(config.subscriptionsGet(0x0002, onOff)).text, "element 0x0002, Generic OnOff Server");
  assert.equal(
    describeMessage(config.publicationSet(0x0002, onOff, { address: 0, appKey: 0, credentials: false, ttl: 0xff, period: 0, retransmitCount: 0, retransmitSteps: 0 })).text,
    "element 0x0002, Generic OnOff Server, publishes nowhere",
  );
  assert.equal(describeMessage(client.ctlSet(0xffff, 6500, -1, 9)).text, "lightness 65535 (100.0 %), 6500 K, delta UV -1, TID 9");
  assert.equal(describeMessage(client.hslSet(0, 0x8000, 0xffff, 2)).text, "lightness 0 (0.0 %), hue 180.0°, saturation 100.0 %, TID 2");
  assert.equal(describeMessage(client.sensorGet(0x004f)).text, "property 0x004F");
  assert.equal(describeMessage(client.faultTest(1, 0x0059)).text, "test 1, company 0x0059");
});

test("key index lists are packed two in three bytes", () => {
  assert.deepEqual(unpackIndexList([0x00, 0x10, 0x00, 0x02, 0x00]), [0, 1, 2]);
  assert.deepEqual(unpackIndexList([]), []);
  assert.equal(describeMessage(hex("8002000000001000")).text, "Success: NetKey 0 has AppKeys 0, 1");
});

test("model clients' messages are as the Mesh Model spec has them", () => {
  assert.equal(asHex(client.onOffSet(true, 7)), "8202" + "0107");
  assert.equal(asHex(client.onOffSet(false, 8, false)), "8203" + "0008");
  assert.equal(asHex(client.levelSet(-1, 1)), "8206ffff01");
  assert.equal(asHex(client.levelDelta(-2, 1)), "8209feffffff01");
  assert.equal(asHex(client.lightnessSet(0xffff, 2)), "824cffff02");
  assert.equal(asHex(client.ctlSet(0x8000, 6500, 0, 3)), "825e0080641900000" + "3");
  assert.equal(asHex(client.hslSet(0x8000, 0x5555, 0xffff, 4)), "827600805555ffff04");
  assert.equal(asHex(client.sensorGet()), "8231");
  assert.equal(asHex(client.sensorGet(0x004f)), "82314f00");
  assert.equal(asHex(client.faultTest(1, 0x0059)), "8032015900");
  assert.equal(asHex(client.attentionSet(5)), "800505");
  assert.equal(answerOf(0x8202), 0x8204);
  assert.equal(answerOf(0x8203), null);
  assert.equal(answerOf(0x8008), 0x02);
});

test("status messages read in words", () => {
  assert.equal(describeMessage(hex("820401")).text, "On");
  assert.equal(describeMessage(hex("820400010a")).text, "Off → On in 1 s");
  assert.equal(describeMessage(hex("8208ffff")).text, "-1");
  assert.equal(describeMessage(hex("824effff")).text, "65535 (100.0 %)");
  assert.equal(describeMessage(hex("82600080641900")).text.split(",")[1], " 6500 K");
  assert.equal(describeMessage(hex("8299aabb")).name, "Opcode 0x8299");
  assert.equal(describeMessage(hex("8299aabb")).text, "AA BB");
});

test("sensor data is read in both of its formats", () => {
  // Format A: property 0x004F, one byte; format B: property 0x1234, two bytes; then one with no value.
  const data = hex("e0092a" + "033412aabb" + "ff3412");
  assert.deepEqual(parseSensorData(data), [
    { property: 0x004f, value: [0x2a] },
    { property: 0x1234, value: [0xaa, 0xbb] },
    { property: 0x1234, value: [] },
  ]);
  assert.equal(
    describeMessage([0x52, ...data]).text,
    "property 0x004F: 2A; property 0x1234: AA BB; property 0x1234: none",
  );
  assert.equal(describeMessage(hex("514f00")).text, "no property 0x004F");
});

test("times, addresses and OOB information read as people say them", () => {
  assert.equal(transitionText(0x0a), "1 s");
  assert.equal(transitionText(0x45), "5 s");
  assert.equal(transitionText(0x8a), "1.7 min");
  assert.equal(transitionText(0x3f), "unknown");
  assert.equal(parseAddress("0xC000"), 0xc000);
  assert.equal(parseAddress("1201"), 0x1201);
  assert.equal(parseAddress("12345"), null);
  assert.equal(addressKind(0x0002), "unicast");
  assert.equal(addressKind(0xc000), "group");
  assert.equal(addressKind(0xffff), "all nodes");
  assert.equal(addressKind(0x8000), "virtual");
  assert.equal(oobInfoText(0x4020), "a number, in the manual");
  assert.equal(oobInfoText(0), "none");
  assert.deepEqual(parseHex("01 0A ff"), [1, 10, 255]);
  assert.equal(parseHex("abc"), null);
});
