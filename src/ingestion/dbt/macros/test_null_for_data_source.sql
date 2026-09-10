{#
  A column the named source never collects must be NULL on that source's rows.

  Zero is the wrong answer there: it states the source measured the thing and
  found none of it, and a class contract that cannot separate "not collected"
  from "measured as none" leaves its consumers guessing. Reads FINAL because
  the class relations are ReplacingMergeTree and parts are not
  duplicate-immune. #3362
#}
{% test null_for_data_source(model, column_name, data_source) %}

SELECT
    {{ column_name }} AS stated_value,
    count() AS offending_rows
FROM {{ model }} FINAL
WHERE data_source = '{{ data_source }}'
  AND {{ column_name }} IS NOT NULL
GROUP BY stated_value

{% endtest %}
