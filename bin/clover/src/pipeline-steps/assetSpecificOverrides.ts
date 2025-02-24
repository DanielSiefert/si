import _ from "npm:lodash";
import _logger from "../logger.ts";
import {
  createInputSocketFromProp,
  ExpandedSocketSpec,
  setAnnotationOnSocket,
} from "../spec/sockets.ts";
import { ExpandedPkgSpec } from "../spec/pkgs.ts";
import { ExpandedPropSpec } from "../spec/props.ts";
import { PropUsageMap } from "./addDefaultPropsAndSockets.ts";

const logger = _logger.ns("assetOverrides").seal();

export function assetSpecificOverrides(
  incomingSpecs: ExpandedPkgSpec[],
): ExpandedPkgSpec[] {
  const newSpecs = [] as ExpandedPkgSpec[];

  for (const spec of incomingSpecs) {
    if (overrides.has(spec.name)) {
      logger.debug(`Running override for ${spec.name}`);
      overrides.get(spec.name)?.(spec);
    }
    newSpecs.push(spec);
  }

  return newSpecs;
}

type OverrideFn = (spec: ExpandedPkgSpec) => void;

const overrides = new Map<string, OverrideFn>([
  ["AWS::EC2::Route", (spec: ExpandedPkgSpec) => {
    // Add GatewayId Socket
    const variant = spec.schemas[0].variants[0];

    const prop = variant.domain.entries.find((p: ExpandedPropSpec) =>
      p.name === "GatewayId"
    );

    if (!prop) return;
    const socket = createInputSocketFromProp(prop);

    setAnnotationOnSocket(socket, { tokens: ["InternetGatewayId"] });
    setAnnotationOnSocket(socket, { tokens: ["VPNGatewayId"] });

    variant.sockets.push(socket);
  }],
  ["AWS::CloudTrail::Trail", (spec: ExpandedPkgSpec) => {
    const variant = spec.schemas[0].variants[0];

    for (const prop of variant.domain.entries) {
      switch (prop.name) {
        case "S3BucketName": {
          const socket = createInputSocketFromProp(prop, [{
            tokens: ["BucketName"],
          }]);

          variant.sockets.push(socket);
          break;
        }
        case "SnsTopicName": {
          const socket = createInputSocketFromProp(prop, [{
            tokens: ["TopicName"],
          }]);

          variant.sockets.push(socket);
          break;
        }
        case "KMSKeyId": {
          const socket = createInputSocketFromProp(prop, [{
            tokens: ["KeyId"],
          }]);

          variant.sockets.push(socket);
          break;
        }
      }
    }
  }],
  [
    "AWS::Route53::HostedZone",
    (spec: ExpandedPkgSpec) => {
      const variant = spec.schemas[0].variants[0];

      // Add an annotation for the Id output socket to connect to HostedZoneId
      const socket = variant.sockets.find(
        (s: ExpandedSocketSpec) => s.name === "Id" && s.data.kind === "output",
      );
      if (!socket) return;

      setAnnotationOnSocket(socket, { tokens: ["HostedZoneId"] });
    },
  ],
  ["ContainerDefinitions Secrets", (spec: ExpandedPkgSpec) => {
    const variant = spec.schemas[0].variants[0];

    const prop = variant.domain.entries.find((p: ExpandedPropSpec) =>
      p.name === "ValueFrom"
    );

    if (!prop) return;
    const socket = createInputSocketFromProp(prop);

    setAnnotationOnSocket(socket, { tokens: ["Id"] });

    variant.sockets.push(socket);
  }],
  [
    "AWS::SecretsManager::Secret",
    addSecretProp("Secret String", "secretString", ["SecretString"]),
  ],
  ["AWS::EC2::Instance", (spec: ExpandedPkgSpec) => {
     const variant = spec.schemas[0].variants[0];
     
     const prop = variant.domain.entries.find((p: ExpandedPropSpec) =>
       p.name === "UserData"
     );
     
     if (!prop) return;
     prop.data.widgetKind = "textarea";
  }],
  [
    "AWS::DataZone::Domain",
    (spec: ExpandedPkgSpec) => {
      const variant = spec.schemas[0].variants[0];

      const socket = variant.sockets.find(
        (s: ExpandedSocketSpec) => s.name === "Id" && s.data.kind === "output",
      );
      if (!socket) return;

      setAnnotationOnSocket(socket, { tokens: ["Domain Identifier"] });
    },
  ],
  [
    "AWS::DataZone::Project",
    (spec: ExpandedPkgSpec) => {
      const variant = spec.schemas[0].variants[0];

      const socket = variant.sockets.find(
        (s: ExpandedSocketSpec) => s.name === "Id" && s.data.kind === "output",
      );
      if (!socket) return;

      setAnnotationOnSocket(socket, { tokens: ["Project Identifier"] });
    },
  ],
]);

function addSecretProp(
  secretKind: string,
  secretKey: string,
  propPath: string[],
) {
  return (spec: ExpandedPkgSpec) => {
    const variant = spec.schemas[0].variants[0];

    const [secretName] = propPath.slice(-1);
    if (!secretName) {
      return;
    }

    // Find secret prop
    let secretParent = variant.domain;
    let secretProp: ExpandedPropSpec | undefined = variant.domain;

    for (const propName of propPath) {
      // If we haven't found the secret prop yet, and we're not with an object in hand, break
      if (secretProp.kind !== "object") {
        secretProp = undefined;
        break;
      }

      secretParent = secretProp;
      const thisProp = secretParent.entries.find((p) => p.name === propName);

      // If we don't find the prop on the parent, break
      if (!thisProp) {
        secretProp = undefined;
        break;
      }

      secretProp = thisProp;
    }

    if (!secretProp) {
      console.log(`Could not add secret value for ${spec.name}`);
      return;
    }

    // Find propUsageMap
    const extraProp = variant.domain.entries.find((p) => p.name === "extra");
    if (extraProp?.kind !== "object") {
      return;
    }
    const propUsageMapProp = extraProp.entries.find((p) =>
      p.name === "PropUsageMap"
    );
    const propUsageMap = JSON.parse(
      propUsageMapProp?.data.defaultValue,
    ) as PropUsageMap;

    if (!propUsageMapProp || !Array.isArray(propUsageMap?.secrets)) {
      return;
    }

    // Remove secret from the domain tree
    secretParent.entries = secretParent.entries.filter((
      p: ExpandedPropSpec,
    ) => p.name !== secretName);

    // Add prop to secrets tree
    secretProp.data.widgetKind = "Secret";
    secretProp.data.widgetOptions = [{
      "label": "secretKind",
      "value": secretKind,
    }];
    variant.secrets.entries.push(secretProp);
    // Replace "domain" with "secrets" on propPath
    secretProp.metadata.propPath[1] = "secrets";

    // Add socket for secret prop
    const secretStringProp = createInputSocketFromProp(secretProp);
    variant.sockets.push(secretStringProp);
    setAnnotationOnSocket(secretStringProp, { tokens: [secretKind] });

    // add secret to the propUsageMap
    propUsageMap.secrets.push({
      secretKey,
      propPath,
    });
    propUsageMapProp.data.defaultValue = JSON.stringify(propUsageMap);
  };
}
