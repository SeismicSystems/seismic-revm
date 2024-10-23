contract SStoreAndLoad {
    uint256 constant SLOT = 0x444;
    uint256 constant VALUE = 0x123456789;

    function test() public {
        uint256 x;
        assembly {
            sstore(SLOT, VALUE)
            x := sload(SLOT)
        }
        assert(x == VALUE);
    }
}