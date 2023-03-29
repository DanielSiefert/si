async function refresh(component) {
    const resource = component.properties.resource?.value;
    if (!resource) {
        return {
            status: component.properties.resource?.status ?? "ok",
            message: component.properties.resource?.message
        };
    }

    let groupId;
    for (const rule of resource) {
        if (groupId !== undefined && groupId !== rule.GroupId) {
            return {
                status: "error",
                value: resource,
                message: "Egress references multiple group ids",
            }
        }
        groupId = rule.GroupId;
    }

    const child = await siExec.waitUntilEnd("aws", [
        "ec2",
        "describe-security-groups",
        "--group-ids",
        groupId,
        "--region",
        component.properties.domain.region,
    ]);

    if (child.stderr.includes("InvalidGroup.NotFound")) {
        console.log(`Group Id: ${groupId}`);
        console.error(child.stderr);
        return {
            status: "error",
            message: `Security Group not found (InvalidGroup.NotFound)`,
        }
    }

    if (child.stderr.includes("InvalidGroupId.Malformed")) {
        console.log(`Group Id: ${groupId}`);
        console.error(child.stderr);
        return {
            status: "error",
            value: resource,
            message: "Security Group Id is invalid (InvalidGroupId.Malformed)",
        }
    }

    if (child.exitCode !== 0) {
        console.log(`Group Id: ${groupId}`);
        console.error(child.stderr);
        return {
            status: "error",
            value: resource,
            message: `AWS CLI 2 "aws ec2 describe-security-groups" returned non zero exit code (${child.exitCode})`,
        }
    }

    const securityGroup = JSON.parse(child.stdout).SecurityGroups[0];
    for (const rule of resource) {
        for (const IpPermission of securityGroup.IpPermissionsEgress) {
            if (IpPermission.ToPort === rule.ToPort &&
                IpPermission.FromPort === rule.FromPort &&
                IpPermission.IpProtocol === rule.IpProtocol) {

                for (const range of IpPermission.IpRanges) {
                    if (range.CidrIp === rule.CidrIpv4) {
                        return {value: resource, status: "ok"};
                    }
                }
            }
        }
    }

    return {status: "error", message: "Egress not found"};
}
