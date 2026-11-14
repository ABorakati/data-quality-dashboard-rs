/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = PERSON
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.PERSON cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = OBSERVATION_PERIOD
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.OBSERVATION_PERIOD cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = VISIT_OCCURRENCE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.VISIT_OCCURRENCE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = VISIT_DETAIL
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.VISIT_DETAIL cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CONDITION_OCCURRENCE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CONDITION_OCCURRENCE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = DRUG_EXPOSURE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.DRUG_EXPOSURE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = PROCEDURE_OCCURRENCE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.PROCEDURE_OCCURRENCE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = DEVICE_EXPOSURE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.DEVICE_EXPOSURE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = MEASUREMENT
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.MEASUREMENT cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = OBSERVATION
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.OBSERVATION cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = DEATH
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.DEATH cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = NOTE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.NOTE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = NOTE_NLP
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.NOTE_NLP cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = SPECIMEN
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.SPECIMEN cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = FACT_RELATIONSHIP
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.FACT_RELATIONSHIP cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = LOCATION
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.LOCATION cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CARE_SITE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CARE_SITE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = PROVIDER
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.PROVIDER cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = PAYER_PLAN_PERIOD
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.PAYER_PLAN_PERIOD cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = COST
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.COST cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = DRUG_ERA
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.DRUG_ERA cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = DOSE_ERA
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.DOSE_ERA cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CONDITION_ERA
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CONDITION_ERA cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = METADATA
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.METADATA cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CDM_SOURCE
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CDM_SOURCE cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CONCEPT
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CONCEPT cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = VOCABULARY
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.VOCABULARY cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = DOMAIN
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.DOMAIN cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CONCEPT_CLASS
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CONCEPT_CLASS cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CONCEPT_RELATIONSHIP
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CONCEPT_RELATIONSHIP cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = RELATIONSHIP
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.RELATIONSHIP cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CONCEPT_SYNONYM
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CONCEPT_SYNONYM cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = CONCEPT_ANCESTOR
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.CONCEPT_ANCESTOR cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = SOURCE_TO_CONCEPT_MAP
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.SOURCE_TO_CONCEPT_MAP cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = DRUG_STRENGTH
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.DRUG_STRENGTH cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

/*********
CDM_TABLE
Verify the table exists.
Parameters used in this template:
schema = main
cdmTableName = COHORT_DEFINITION
**********/
SELECT 
    num_violated_rows, 
    CASE 
        WHEN denominator.num_rows = 0 THEN 0 
        ELSE 1.0*num_violated_rows/denominator.num_rows 
    END AS pct_violated_rows, 
    denominator.num_rows AS num_denominator_rows
FROM
(
    SELECT 
        num_violated_rows 
    FROM
    (
        SELECT
            CASE 
                WHEN COUNT(*) = 0 THEN 0
                ELSE 0
            END AS num_violated_rows
        FROM main.COHORT_DEFINITION cdmTable
    ) violated_rows
) violated_row_count,
( 
    SELECT 1 AS num_rows
) denominator
;

