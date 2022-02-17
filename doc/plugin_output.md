# Details on the plugin output

## Default output

Specifying a value for the plugin output in a mapping is optional. When no value is provided different default outputs are sent to the Icinga API, depending on the overall exit status of the check, whether the passive check result updates an Icinga host or service object and the number of result vectors returned by the PromQL query.


**Defaults for Icinga services**

Case #1: PromQL query returned a single item.

exit value | plugin output | example
--- | --- | ---
0 | [OK] PromQL query returned one result (\<value\>) | [OK] PromQL query returned one result (5.56)
1 | [WARNING] PromQL query returned one result within the warning range (\<value\> in \<range\>) | [WARNING] PromQL query returned one result within the warning range (12.15 in @10:20)
2 | [CRITICAL] PromQL query returned one result within the critical range  (\<value\> in \<range\>) | [CRITICAL] PromQL query returned one result within the critical range (22.86 in @0:30)

Case #2: PromQL query returned multiple items.

exit value | plugin output | example
--- | --- | ---
0 | [OK] PromQL query returned multiple results in the range \<min\>..=\<max\> | [OK] PromQL query returned multiple results in the range 0.05..=6.74
1 | [WARNING] PromQL query returned multiple results within the warning range (values \<min\>..=\<max\> overlap with \<range\> | [WARNING] PromQL query returned multiple results within the warning range (values 12..=20 overlap with @10:20)
1 | [CRITICAL] PromQL query returned multiple results within the critical range (values \<min\>..=\<max\> overlap with \<range\> | [CRITICAL] PromQL query returned multiple results within the critical range (values 75.50..=110 overlap with @0:100)

Case #3: PromQL query returned no result at all

exit status | plugin output
--- | ---
3 | [UNKNOWN] PromQL query result set is empty


**Defaults for Icinga hosts**

The output for host objects will look almost the same. Only the service/host state in the square brackets above will be mapped accordingly:

* exit value 0 -> [UP] ...
* exit value 1 -> [UP] ...
* exit value 2 -> [DOWN] ...




