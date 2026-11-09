#include "MiniBpm.h"
#include <algorithm>
#include <cmath>

extern "C" double lemon_minibpm(const float *left, const float *right, int count, float rate) noexcept {
    try {
        breakfastquay::MiniBPM detector(rate);
        detector.setBPMRange(55.0, 210.0);
        float mono[4096];
        for (int start = 0; start < count; start += 4096) {
            const int size = std::min(4096, count - start);
            for (int i = 0; i < size; ++i) {
                const float l = left[start + i], r = right[start + i];
                mono[i] = 0.5f * ((std::isfinite(l) ? l : 0.0f) + (std::isfinite(r) ? r : 0.0f));
            }
            detector.process(mono, size);
        }
        return detector.estimateTempo();
    } catch (...) {
        return 0.0;
    }
}
