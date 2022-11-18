SELECT
    child_prop_ids.child_prop_ids as child_prop_ids,
    row_to_json(props.*)          AS object
FROM props_v1($1, $2) AS props
LEFT JOIN (
    SELECT
        prop_belongs_to_prop.belongs_to_id        AS belongs_to_id,
        array_agg(prop_belongs_to_prop.object_id) AS child_prop_ids
    FROM prop_belongs_to_prop_v1($1, $2) AS prop_belongs_to_prop
    GROUP BY prop_belongs_to_prop.belongs_to_id
) AS child_prop_ids
    ON child_prop_ids.belongs_to_id = props.id
WHERE
    props.hidden = FALSE
    AND props.id IN (
        WITH RECURSIVE recursive_props AS (
            SELECT left_object_id AS prop_id
            FROM prop_many_to_many_schema_variants_v1($1, $2) AS prop_many_to_many_schema_variants
            WHERE right_object_id = $3
            UNION ALL
            SELECT pbp.object_id AS prop_id
            FROM prop_belongs_to_prop_v1($1, $2) AS pbp
            JOIN recursive_props ON pbp.belongs_to_id = recursive_props.prop_id
        )
        SELECT prop_id
        FROM recursive_props
    )
