/* Compile as both C and C++ against the public header and actual shared library. */
#include "latexsnipper_session.h"
#include <stdio.h>
#include <string.h>

#define HASH_A "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
static const char catalog[] =
    "{\"commands\":{\"scope\":{\"userSha256\":\"" HASH_A
    "\",\"projectSha256\":null,\"documentSha256\":null,\"generation\":\"42\"},\"definitions\":[]},"
    "\"policy\":{\"retrievalEnabled\":true,\"sensitiveDocument\":false,\"allowInSensitiveDocument\":false},"
    "\"fragments\":[{\"id\":\"saved\",\"revision\":\"42\",\"userSha256\":\"" HASH_A
    "\",\"projectSha256\":null,\"documentSha256\":null,\"trigger\":\"bayes\","
    "\"label\":\"Saved formula\",\"source\":\"\\\\frac{}{}\"}]}";

static int expect_and_free(char *response, const char *needle) {
    int matched = response != NULL && strstr(response, needle) != NULL;
    if (response != NULL) latexsnipper_string_free(response);
    return matched;
}

int main(void) {
    char request[4096];
    char snapshot[512];
    char *response;
    const char *start;
    const char *end;
    size_t length;
    int written;
    if (!expect_and_free(latexsnipper_formula_fragment_completion_capabilities(), "\"schemaVersion\":3")) return 1;
    if (!expect_and_free(latexsnipper_formula_completion_capabilities(), "\"schemaVersion\":1")) return 2;
    if (!expect_and_free(latexsnipper_formula_scoped_completion_capabilities(), "\"schemaVersion\":2")) return 3;
    if (!expect_and_free(latexsnipper_formula_complete_fragments(NULL, 0), "INVALID_ARGUMENT")) return 4;
    written = snprintf(request, sizeof(request),
        "{\"source\":\"\\\\bay\",\"inputFormat\":\"latex\",\"cursor\":4,\"revision\":\"42\",\"catalog\":%s}", catalog);
    if (written < 0 || (size_t)written >= sizeof(request)) return 5;
    response = latexsnipper_formula_complete_fragments((const uint8_t *)request, (size_t)written);
    if (response == NULL) return 6;
    /* This fixed schema's snapshot contains only primitive fields, no nested JSON. */
    start = strstr(response, "\"snapshot\":{");
    if (start == NULL) { latexsnipper_string_free(response); return 7; }
    start += strlen("\"snapshot\":");
    end = strchr(start, '}');
    if (end == NULL) { latexsnipper_string_free(response); return 8; }
    length = (size_t)(end - start) + 1;
    if (length >= sizeof(snapshot)) { latexsnipper_string_free(response); return 9; }
    memcpy(snapshot, start, length); snapshot[length] = '\0';
    latexsnipper_string_free(response);
    written = snprintf(request, sizeof(request),
        "{\"source\":\"\\\\bay\",\"inputFormat\":\"latex\",\"cursor\":4,\"revision\":\"42\",\"catalog\":%s,\"snapshot\":%s,\"candidateId\":\"fragment:saved\"}", catalog, snapshot);
    if (written < 0 || (size_t)written >= sizeof(request)) return 10;
    if (!expect_and_free(latexsnipper_formula_completion_apply_fragments((const uint8_t *)request, (size_t)written), "\"source\":\"\\\\frac{}{}\"")) return 11;
    puts("Public C/C++ fragment ABI: capabilities, query, guarded apply and string ownership passed");
    return 0;
}
